#!/usr/bin/env python
"""Build a correctly segmented Ghidra program for ACAD.EXE.

Blocks and addresses come from build/ovl-map.json, which acad-re generates.
This script decodes no 1983 file formats of its own.

`prepare()` does the work on an already-open program; `main()` opens one around
it. Keeping the two apart is what lets export_ast.py analyse and export inside a
single `with pyghidra.open_program(...)` block — the program is closed when that
block exits, so it cannot be returned and used afterwards.
"""

import hashlib
import json
import os
import shutil
import sys
import tempfile
from analysis_layout import normalize_layout

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ghidra_common as gc  # noqa: E402

gc.start()

import pyghidra  # noqa: E402
from java.math import BigInteger  # noqa: E402


def read_map(map_path):
    with open(map_path) as fh:
        return normalize_layout(json.load(fh))


def set_segment_registers(ctx, block, code_seg, data_seg):
    """Tell the disassembler what CS/DS/SS/ES are over one block.

    Without this every data reference in the block resolves to the wrong place,
    because the program does its own segment arithmetic and carries no
    relocations for Ghidra to infer from.
    """
    for reg_name, value in (
        ("CS", code_seg),
        ("DS", data_seg),
        ("SS", data_seg),
        ("ES", data_seg),
    ):
        reg = ctx.getRegister(reg_name)
        if reg is not None:
            ctx.setValue(reg, block.getStart(), block.getEnd(), BigInteger.valueOf(value))


def prepare(program, m):
    """Split the EXE into code and data, set the segment registers, verify."""
    normalized = normalize_layout(m)
    if normalized is not m:
        m.clear()
        m.update(normalized)
    with open(m["exe"], "rb") as fh:
        header = fh.read(32)
    if header[:2] != b"MZ" or header[20:24] != bytes.fromhex("0001f0ff"):
        raise ValueError("native-IP normalization requires the recovered MZ CS/IP")
    exe_code = next(b for b in m["blocks"] if b["name"] == "EXE_CODE")
    exe_data = next(b for b in m["blocks"] if b["name"] == "EXE_DATA")
    mem = program.getMemory()

    # Ghidra's MZ loader makes one flat block. Split it so that code and data
    # are separately addressable, then tell the disassembler what DS is —
    # without that, every string reference resolves to the wrong place.
    flat = gc.block_by_name(program, "CODE_0")
    if flat is not None:
        wanted_start = gc.seg_addr(program, exe_code["seg"], exe_code["off"])
        delta = wanted_start.subtract(flat.getStart())
        if delta:
            base = int(program.getImageBase().getOffset()) + int(delta)
            if base & 15:
                raise ValueError("normalized image base is not paragraph-aligned")
            program.setImageBase(gc.seg_addr(program, base >> 4, 0), True)
        split_at = gc.seg_addr(program, exe_data["seg"], 0)
        mem.split(flat, split_at)
        mem.getBlock(flat.getStart()).setName("EXE_CODE")
        mem.getBlock(split_at).setName("EXE_DATA")

    ctx = program.getProgramContext()
    code = gc.block_by_name(program, "EXE_CODE")
    set_segment_registers(ctx, code, exe_code["seg"], exe_data["seg"])

    from ghidra.util.task import ConsoleTaskMonitor

    made = add_overlay_blocks(program, m, ConsoleTaskMonitor())
    assert made == 22, "expected 22 overlay regions, made %d" % made

    # Overlay code needs the segment registers just as much as the EXE's does,
    # and the blocks only exist now. Overlay code runs with the same DS: its
    # data pages into the EXE's data segment, and it reaches the EXE's strings
    # there too. Without this, 69 of the 151 analysed functions would be
    # disassembled with an unknown DS and their data references would not
    # resolve.
    for block in program.getMemory().getBlocks():
        if block.getName().startswith("OVL") and block.getName().endswith("_CODE"):
            overlay_spec = next(b for b in m["blocks"] if b["name"] == block.getName())
            set_segment_registers(ctx, block, overlay_spec["seg"], exe_data["seg"])

    seeded = seed_entry_points(program, m)
    assert seeded == 11, "expected 11 overlay entry points, seeded %d" % seeded

    check(program, exe_data["seg"])


def add_overlay_blocks(program, m, monitor):
    """One Ghidra overlay block per region.

    The 11 entries page into the same two windows, so their bytes cannot
    coexist in one flat address space. Ghidra overlay blocks are exactly this:
    several blocks claiming one address range, each in its own space.
    """
    import jpype

    mem = program.getMemory()
    with open(m["ovl"], "rb") as fh:
        ovl = fh.read()

    made = 0
    for b in m["blocks"]:
        if not b["overlay"]:
            continue
        chunk = ovl[b["src_off"] : b["src_off"] + b["len"]]
        if len(chunk) != b["len"]:
            raise SystemExit(
                "%s wants %d bytes at %#x, file has %d"
                % (b["name"], b["len"], b["src_off"], len(chunk))
            )
        start = gc.seg_addr(program, b["seg"], b["off"])
        try:
            block = mem.createInitializedBlock(b["name"], start, b["len"], 0, monitor, True)
        except jpype.JException as exc:
            raise SystemExit("overlay block %s could not be created: %s" % (b["name"], exc))
        # Java bytes are signed; values >= 0x80 must be sign-extended or JPype
        # rejects the array.
        mem.setBytes(
            block.getStart(),
            jpype.JArray(jpype.JByte)([c - 256 if c > 127 else c for c in chunk]),
        )
        block.setExecute(b["name"].endswith("_CODE"))
        made += 1
    print("created %d overlay blocks" % made)
    return made


def seed_entry_points(program, m):
    """Mark each overlay's entry point as a function before analysis.

    Ghidra finds functions by following flow from somewhere it already knows is
    code. An overlay block has no such anchor, so without this it only stumbles
    on whatever a data reference happens to point at — 4 of the 11 code blocks
    had any functions at all, and 7 had none. The directory records where
    `ACAD.EXE` far-calls into each overlay (`entry+0x10`), which is exactly the
    anchor analysis needs.
    """
    from ghidra.app.cmd.disassemble import DisassembleCommand
    from ghidra.app.cmd.function import CreateFunctionCmd

    blocks = {b["name"]: b for b in m["blocks"] if b["overlay"]}
    seeded = 0
    for ep in m["entry_points"]:
        name = "OVL%02d_CODE" % ep["entry"]
        spec = blocks.get(name)
        block = gc.block_by_name(program, name)
        if spec is None or block is None:
            continue
        delta = ep["off"] - spec["off"]
        if delta < 0 or delta >= spec["len"]:
            print("entry %d: entry point %#x is outside %s" % (ep["entry"], ep["off"], name))
            continue
        addr = block.getStart().add(delta)
        DisassembleCommand(addr, None, True).applyTo(program)
        CreateFunctionCmd("ovl%02d_entry" % ep["entry"], addr, None, None).applyTo(program)
        seeded += 1
    print("seeded %d overlay entry points" % seeded)
    return seeded


def check(program, data_seg):
    """The layout is right only if known strings read back at known addresses.

    DS:0x3553 is the first byte of ACAD.EXE's string pool and DS:0x3597 is the
    overlay filename format (spec §4.1). Both are load-bearing: if the data
    segment were off by even one paragraph, neither would match.
    """
    for off, want in ((0x3553, b"Not enough core for overlays"), (0x3597, b"%c:%s.OVL")):
        addr = gc.seg_addr(program, data_seg, off)
        got = gc.read_bytes(program, addr, len(want))
        assert got == want, "DS:%04x is %r, expected %r" % (off, got, want)
    print("segment layout verified: DS:3553 and DS:3597 read back correctly")

    # The overlay payload must land where the directory says. `!root` sits at
    # file offset 0x5e00, which is entry 0's data region start, so it must be
    # the first thing in OVL00_DATA.
    block = gc.block_by_name(program, "OVL00_DATA")
    got = gc.read_bytes(program, block.getStart(), 5)
    assert got == b"!root", "OVL00_DATA starts with %r, expected b'!root'" % got
    print("overlay payload verified: OVL00_DATA begins at the !root token")


def project_location(project_dir, fresh=False):
    """An existing absolute directory Ghidra will accept as a project location.

    `fresh` removes any previous project first. The pipeline always passes it:
    a project carried over from an earlier `ovl-map.json` would keep stale
    overlay blocks, and re-creating a block that already exists is an error.

    Ghidra's ProjectLocator rejects any path element starting with '.', so a
    checkout under a dot-directory — ~/.local/src/..., or a git worktree under
    .claude/worktrees/ — cannot hold its own project. Fall back to a stable
    directory under the system temp dir, keyed by the repo path so two checkouts
    never share one project, and say where it went. The project is derived data;
    only its location moves.
    """
    path = os.path.abspath(project_dir)
    if not any(part.startswith(".") for part in path.split(os.sep) if part):
        if fresh:
            shutil.rmtree(path, ignore_errors=True)
        os.makedirs(path, exist_ok=True)
        return path

    key = hashlib.sha1(path.encode()).hexdigest()[:12]
    fallback = os.path.join(tempfile.gettempdir(), "autorust-ghidra", key)
    if fresh:
        shutil.rmtree(fallback, ignore_errors=True)
    os.makedirs(fallback, exist_ok=True)
    print("ghidra project: %s is under a dot-directory, which Ghidra rejects; using %s" % (path, fallback))
    return fallback


def main(map_path, project_dir):
    m = read_map(map_path)
    with pyghidra.open_program(
        m["exe"],
        project_location=project_location(project_dir),
        project_name="acad14",
        analyze=False,
    ) as api:
        prepare(api.getCurrentProgram(), m)
    print("loaded ACAD.EXE")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
