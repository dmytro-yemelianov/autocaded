"""Recover compiler inline operands without treating them as x86 instructions.

The decoder operates on an already-open, normalized Ghidra program. It does
not change its listing. Shared-window byte candidates retain their provenance
and a conditional status until the loader's residency contract is established.
"""
from collections import deque
from pathlib import Path


SWITCH = bytes.fromhex(
    "5f58068ccb8ec3268b0d8bd9d1e303fb41fdf2af0783c70403fb2e03bd0000ffe7"
)


def helper_kind(data):
    if data.startswith(bytes.fromhex("5e33c0eb06")):
        return "frame", 0
    # The two stack-limit address bytes may differ across compiler images.
    if (data[:13] == bytes.fromhex("5efc2eacb400558bec2be03b26")
            and data[15:19] == bytes.fromhex("76e1ffe6")):
        return "frame", 1
    if data.startswith(bytes.fromhex("5efc2eadebed")):
        return "frame", 2
    if data.startswith(SWITCH):
        return "switch", None
    if data.startswith(bytes.fromhex("5e2e8b34ff2e764d")):
        return "kernel-tailcall", 2
    return None


def switch_table(data, offset):
    """Recover the default/case relative targets consumed by SWITCH above."""
    if len(data) < 2:
        raise ValueError("truncated switch count")
    count = int.from_bytes(data[:2], "little")
    length = 4 * count + 4
    if count > 1024 or len(data) < length or offset + length > 65536:
        raise ValueError("truncated, wrapping, or oversized switch table")
    values = [int.from_bytes(data[2 + 2*i:4 + 2*i], "little")
              for i in range(count)]
    if len(set(values)) != count:
        raise ValueError("duplicate switch values")
    targets = []
    for i in range(count + 1):
        word = 2 + 2*count + 2*i
        delta = int.from_bytes(data[word:word+2], "little", signed=True)
        targets.append({"case": None if i == 0 else values[i-1],
                        "target": (offset + word + delta) & 65535})
    return length, targets


def directory_views(spec):
    """Identify byte-identical code AND data subranges of a larger entry."""
    blocks = {b["name"]: b for b in spec["blocks"]}
    views = {}
    for name, child in blocks.items():
        if not name.startswith("OVL") or not name.endswith("_CODE"):
            continue
        data = blocks[name.replace("_CODE", "_DATA")]
        for other, parent in blocks.items():
            if other == name or not other.startswith("OVL") or not other.endswith("_CODE"):
                continue
            parent_data = blocks[other.replace("_CODE", "_DATA")]
            def contains(p, c):
                return (p["seg"] == c["seg"] and p["off"] <= c["off"]
                        and c["off"] + c["len"] <= p["off"] + p["len"]
                        and c["src_off"] - p["src_off"] == c["off"] - p["off"])
            if parent["len"] > child["len"] and contains(parent, child) and contains(parent_data, data):
                views[name] = other
                break
    return views


def recover(program, spec, exporter):
    from ghidra.app.util import PseudoDisassembler

    blocks = {str(b.getName()): b for b in program.getMemory().getBlocks()}
    maps = {b["name"]: b for b in spec["blocks"]}
    views = directory_views(spec)
    exe = Path(spec["exe"]).read_bytes()
    ovl = Path(spec["ovl"]).read_bytes()
    mz = int.from_bytes(exe[8:10], "little") * 16
    decoder = PseudoDisassembler(program)

    def file_offset(name, off):
        b = maps[name]
        start = (b["src_off"] if b["overlay"] else
                 mz + (b["seg"] - spec.get("image_load_segment", 4096))*16
                 + b["off"])
        return start + off - b["off"]

    def bytes_at(name, off, size):
        b = maps[name]
        if not b["off"] <= off < b["off"] + b["len"]:
            return b""
        size = min(size, b["off"] + b["len"] - off)
        start = file_offset(name, off)
        return (ovl if b["overlay"] else exe)[start:start+size]

    # These wrappers pass concrete directory entries into the entry-2 loader.
    # It writes only that entry's low code range, then near-calls its entry
    # offset under the same CS. The high entry-2 window is retained on this
    # dispatch path. Require the actual recovered code before applying it.
    shared_dispatch = {}
    dispatch = bytes_at("OVL02_CODE", 0x5ba8, 66)
    loader_thunk = bytes_at("OVL02_CODE", 0x50bf, 5)
    common = maps["OVL02_CODE"]
    kernel_loader = bytes_at("EXE_CODE", 0x965d - spec.get("resident_ip_bias", 0), 224)
    for index, wrapper in ((3, 0x5b38), (4, 0x5b69), (5, 0x5b7e),
                           (6, 0x5b93), (7, 0x5b4d)):
        name = "OVL%02d_CODE" % index
        block = maps[name]
        directory_offset = 13 + 18*index
        selection = bytes.fromhex("8d369f5081c6") + directory_offset.to_bytes(2, "little")
        if (selection in bytes_at("OVL02_CODE", wrapper, 24)
                and bytes.fromhex("e8e5f4") in dispatch
                and bytes.fromhex("8b3611218b741089f0ffd0") in dispatch
                and loader_thunk == bytes.fromhex("e874015d96")
                and bytes.fromhex("ff7406ff74048b7604ff74028b7604ff34e84b00") in kernel_loader
                and block["off"] + block["len"] <= common["off"]):
            shared_dispatch[name] = {"wrapper": wrapper,
                                     "directory_offset": directory_offset,
                                     "dispatcher": 0x5ba8,
                                     "kernel_loader_native_ip": 0x965d,
                                     "scope": "entry-2 dispatch path"}

    def candidate(name, off):
        data = bytes_at(name, off, len(SWITCH))
        kind = helper_kind(data)
        if kind:
            return {"block": name, "offset": off, "kind": kind,
                    "status": "same-block", "signature_bytes": data.hex()}
        # Entry 2 covers the high common window. This is deliberately a
        # candidate, not a claim that entry 2 is resident for another entry.
        if name.startswith("OVL") and name != "OVL02_CODE":
            data = bytes_at("OVL02_CODE", off, len(SWITCH))
            kind = helper_kind(data)
            if kind:
                return {"block": "OVL02_CODE", "offset": off, "kind": kind,
                        "status": ("recovered-common-window" if name in shared_dispatch
                                   else "conditional-common-window"),
                        "dispatch_proof": shared_dispatch.get(name),
                        "signature_bytes": data.hex()}
        return None

    seeds = []
    for f in program.getFunctionManager().getFunctions(True):
        b = program.getMemory().getBlock(f.getEntryPoint())
        if b is None or str(b.getName()) not in maps:
            continue
        name = str(b.getName())
        if not (name == "EXE_CODE" or name.endswith("_CODE")):
            continue
        off = maps[name]["off"] + int(f.getEntryPoint().subtract(b.getStart()))
        seeds.append((name, off))
    seeds.extend(("OVL%02d_CODE" % e["entry"], e["off"])
                 for e in spec["entry_points"])
    bridge_bytes = bytes_at("EXE_DATA", 0x4d76, 2)
    bridge_ip = int.from_bytes(bridge_bytes, "little")
    bridge = bridge_ip - spec.get("resident_ip_bias", 0)
    bridge_signature = bytes.fromhex("8b1e864d4b4b8f877e4d781c")
    bridge_verified = bytes_at("EXE_CODE", bridge, len(bridge_signature)) == bridge_signature
    if bridge_verified:
        seeds.append(("EXE_CODE", bridge))
    queue = deque(dict.fromkeys(seeds))
    done = set()
    functions = []
    all_inline = {}
    while queue:
        name, entry = queue.popleft()
        name = views.get(name, name)
        if (name, entry) in done:
            continue
        done.add((name, entry))
        b = maps[name]
        if not b["off"] <= entry < b["off"] + b["len"]:
            continue
        pending = deque([entry])
        rows, edges, inline, dependencies, errors = {}, [], [], [], []
        covered = {}
        while pending:
            off = pending.popleft()
            if off in rows:
                continue
            if not b["off"] <= off < b["off"] + b["len"]:
                edges.append({"kind": "outside-block", "to": off})
                continue
            if off in covered:
                errors.append({"offset": off, "error": "flow enters instruction or inline operand"})
                continue
            if len(rows) >= 8192:
                errors.append({"offset": off, "error": "function instruction cap"})
                break
            address = blocks[name].getStart().add(off - b["off"])
            try:
                ins = decoder.disassemble(address)
                if ins is None:
                    raise ValueError("decoder returned no instruction")
                length = int(ins.getLength())
                data = bytes_at(name, off, length)
                if len(data) != length or any(p in covered for p in range(off, off+length)):
                    raise ValueError("overlapping or truncated instruction")
                if bytes(int(v) & 255 for v in ins.getBytes()) != data:
                    raise ValueError("decoded program bytes differ from original input")
                row = {"offset": off, "address": str(address), "bytes": data.hex(),
                       "file": "ACAD.OVL" if b["overlay"] else "ACAD.EXE",
                       "file_offset": file_offset(name, off),
                       "instruction": str(ins),
                       "ops": [{"op": str(o.getMnemonic()),
                                "out": exporter.varnode(o.getOutput()),
                                "in": [exporter.varnode(o.getInput(i))
                                       for i in range(o.getNumInputs())]}
                               for o in ins.getPcode()]}
                rows[off] = row
                for p in range(off, off+length):
                    covered[p] = off
                ft = ins.getFlowType()
                next_off = off + length if ins.getFallThrough() is not None else None
                if data[0] == 0xe8 and length == 3:
                    target = (off + 3 + int.from_bytes(data[1:], "little", signed=True)) & 65535
                    helper = candidate(name, target)
                    target_block = (name if b["off"] <= target < b["off"] + b["len"]
                                    else "OVL02_CODE" if name in shared_dispatch
                                    and common["off"] <= target < common["off"] + common["len"]
                                    else None)
                    edges.append({"from": off, "kind": "call", "to": target,
                                  "block": target_block, "helper": helper,
                                  "status": "resolved-static-window" if target_block
                                  else "unresolved-window"})
                    if target_block:
                        queue.append((target_block, target))
                    if helper:
                        dependencies.append(helper)
                        kind, width = helper["kind"]
                        if kind == "frame":
                            payload = bytes_at(name, off+3, width) if width else b""
                            if len(payload) != width:
                                raise ValueError("truncated inline frame size")
                            row["compiler_frame"] = {
                                "local_bytes": int.from_bytes(payload, "little"),
                                "continuation": off+3+width,
                                "status": helper["status"]}
                            next_off = off+3+width
                            if width:
                                inline.append({"offset": off+3, "bytes": payload.hex(),
                                               "kind": "frame-size", "call": off,
                                               "status": helper["status"]})
                        elif kind == "switch":
                            length_table, targets = switch_table(bytes_at(name, off+3, 4100), off+3)
                            payload = bytes_at(name, off+3, length_table)
                            inline.append({"offset": off+3, "bytes": payload.hex(),
                                           "kind": "switch-table", "call": off,
                                           "status": helper["status"]})
                            for t in targets:
                                edges.append(dict(t, **{"from": off, "kind": "switch",
                                                       "status": helper["status"]}))
                                pending.append(t["target"])
                            next_off = None
                        else:
                            payload = bytes_at(name, off+3, 2)
                            if len(payload) != 2:
                                raise ValueError("truncated kernel function operand")
                            native_ip = int.from_bytes(payload, "little")
                            logical = native_ip - spec.get("resident_ip_bias", 0)
                            inline.append({"offset": off+3, "bytes": payload.hex(),
                                           "kind": "kernel-function", "call": off,
                                           "status": helper["status"]})
                            row["kernel_tailcall"] = {"native_ip": native_ip,
                                                      "target": logical,
                                                      "block": "EXE_CODE"}
                            edges.append({"from": off, "kind": "kernel-tailcall",
                                          "block": "EXE_CODE", "to": logical,
                                          "status": "static-dispatch-contract"})
                            queue.append(("EXE_CODE", logical))
                            next_off = None
                        for item in inline[-1:]:
                            if item["call"] == off:
                                for p in range(item["offset"], item["offset"]+len(bytes.fromhex(item["bytes"]))):
                                    if p in covered:
                                        raise ValueError("inline operand overlaps instruction")
                                    covered[p] = off
                elif ft.isJump():
                    targets = list(ins.getFlows())
                    for target in targets:
                        linear = int(target.getOffset())
                        logical = linear - b["seg"]*16
                        if 0 <= logical <= 65535:
                            edges.append({"from": off, "kind": "branch", "to": logical})
                            pending.append(logical)
                        else:
                            edges.append({"from": off, "kind": "external-branch", "to": str(target)})
                    if not targets:
                        edges.append({"from": off, "kind": "indirect-branch", "to": None})
                elif ft.isCall():
                    edges.append({"from": off, "kind": "indirect-or-far-call",
                                  "to": [str(x) for x in ins.getFlows()]})
                if next_off is not None:
                    pending.append(next_off)
            except Exception as exc:
                errors.append({"offset": off, "error": str(exc)})
        for item in inline:
            all_inline[(name, item["offset"])] = item
        functions.append({"block": name, "entry": entry,
                          "instructions": [rows[x] for x in sorted(rows)],
                          "edges": edges, "inline_data": inline,
                          "dependencies": list({(x["block"], x["offset"], x["status"]): x
                                                for x in dependencies}.values()),
                          "errors": errors})
    # Old automatic seeds inside newly recovered inline data are false entries.
    rejected, retained = [], []
    for f in functions:
        reason = next((v for (name, off), v in all_inline.items()
                       if name == f["block"] and off <= f["entry"] < off+len(bytes.fromhex(v["bytes"]))), None)
        if reason:
            rejected.append({"block": f["block"], "entry": f["entry"], "inline": reason})
        else:
            retained.append(f)
    language = program.getLanguage()
    architecture = {
        "language": str(language.getLanguageID()),
        "ram_space_id": int(program.getAddressFactory().getDefaultAddressSpace().getSpaceID()),
        "userops": [str(language.getUserDefinedOpName(i))
                    for i in range(language.getNumberOfUserDefinedOpNames())],
    }
    return {"architecture": architecture,
            "functions": retained, "rejected_inline_seeds": rejected,
            "directory_views": views,
            "shared_dispatch_proofs": shared_dispatch,
            "kernel_bridge": {"native_ip": bridge_ip, "resident_offset": bridge,
                              "signature_verified": bridge_verified,
                              "offset_slot": "DS:4D76", "segment_slot": "DS:4D78"},
            "limitations": ["Conditional shared-window dependencies are not a runtime residency proof.",
                            "Indirect branches and far calls remain explicit unresolved edges."]}
