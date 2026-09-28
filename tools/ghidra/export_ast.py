#!/usr/bin/env python
"""Export both AST forms from the analysed program (spec §6.3).

  ast-pcode.json  high P-Code, SSA form, language-neutral
  ast-clang.json  Clang markup, a C AST whose nodes link back to P-Code varnodes

Functions Ghidra cannot decompile are recorded in `failures` with the reason.
Dropping them would inflate the §8 gate's success rate and decide it wrongly.
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ghidra_common as gc  # noqa: E402
import load_acad  # noqa: E402

gc.start()

import pyghidra  # noqa: E402

BAD_MARKERS = ("halt_baddata", "UNRECOVERED_JUMPTABLE")


def varnode(vn):
    if vn is None:
        return None
    return {
        "space": str(vn.getAddress().getAddressSpace().getName()),
        "offset": int(vn.getOffset()),
        "size": int(vn.getSize()),
        "unique": bool(vn.isUnique()),
    }


def pcode_ops(high):
    ops = []
    it = high.getPcodeOps()
    while it.hasNext():
        op = it.next()
        ops.append(
            {
                "seq": str(op.getSeqnum().getTarget()),
                "op": str(op.getMnemonic()),
                "out": varnode(op.getOutput()),
                "in": [varnode(op.getInput(i)) for i in range(op.getNumInputs())],
            }
        )
    return ops


def clang_nodes(markup):
    """Flatten the C AST, keeping each node's type and its P-Code linkage."""
    out = []

    def walk(node, depth):
        entry = {"depth": depth, "class": type(node).__name__, "text": str(node)}
        getter = getattr(node, "getPcodeOp", None)
        op = getter() if getter is not None else None
        if op is not None:
            entry["pcode"] = str(op.getSeqnum().getTarget())
        out.append(entry)
        for i in range(node.numChildren()):
            walk(node.Child(i), depth + 1)

    walk(markup, 0)
    return out


def export(program, out_dir):
    from ghidra.app.decompiler import DecompInterface
    from ghidra.util.task import ConsoleTaskMonitor

    dec = DecompInterface()
    dec.toggleCCode(True)
    dec.toggleSyntaxTree(True)
    dec.openProgram(program)
    monitor = ConsoleTaskMonitor()

    pcode = {"functions": [], "failures": []}
    clang = {"functions": [], "failures": []}

    for func in program.getFunctionManager().getFunctions(True):
        key = str(func.getEntryPoint())
        block = program.getMemory().getBlock(func.getEntryPoint())
        common = {
            "address": key,
            "name": str(func.getName()),
            "block": str(block.getName()) if block is not None else None,
        }
        res = dec.decompileFunction(func, 120, monitor)
        if not res.decompileCompleted():
            reason = str(res.getErrorMessage() or "decompile did not complete")
            pcode["failures"].append(dict(common, reason=reason))
            clang["failures"].append(dict(common, reason=reason))
            continue

        c_text = str(res.getDecompiledFunction().getC())
        markers = [m for m in BAD_MARKERS if m in c_text]

        high = res.getHighFunction()
        if high is None:
            pcode["failures"].append(dict(common, reason="no HighFunction"))
        else:
            pcode["functions"].append(dict(common, markers=markers, ops=pcode_ops(high)))

        markup = res.getCCodeMarkup()
        if markup is None:
            clang["failures"].append(dict(common, reason="no C markup"))
        else:
            clang["functions"].append(dict(common, markers=markers, nodes=clang_nodes(markup)))

    for name, doc in (("ast-pcode.json", pcode), ("ast-clang.json", clang)):
        path = os.path.join(out_dir, name)
        with open(path, "w") as fh:
            json.dump(doc, fh, indent=1)
        print(
            "wrote %s (%d functions, %d failures)"
            % (path, len(doc["functions"]), len(doc["failures"]))
        )
    return pcode, clang


def main(map_path, project_dir, out_dir):
    m = load_acad.read_map(map_path)
    with pyghidra.open_program(
        m["exe"],
        project_location=load_acad.project_location(project_dir, fresh=True),
        project_name="acad14",
        analyze=False,
    ) as api:
        program = api.getCurrentProgram()
        load_acad.prepare(program, m)
        api.analyzeAll(program)
        print("analysis complete: %d functions" % program.getFunctionManager().getFunctionCount())
        export(program, out_dir)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
