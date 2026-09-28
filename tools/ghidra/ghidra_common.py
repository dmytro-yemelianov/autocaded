"""Helpers shared by the PyGhidra scripts. Import after pyghidra.start()."""

import os

import pyghidra

_started = False


def start():
    """Boot the JVM against the Ghidra located by tools/ghidra-env.sh.

    Idempotent: export_ast imports load_acad, which starts the JVM at import
    time, and would otherwise start it a second time.
    """
    global _started
    if not _started:
        pyghidra.start(install_dir=os.environ["GHIDRA_INSTALL_DIR"])
        _started = True


def seg_addr(program, seg, off):
    """A real-mode seg:off address in the program's default space."""
    return program.getAddressFactory().getAddress("%04x:%04x" % (seg, off))


def block_by_name(program, name):
    for b in program.getMemory().getBlocks():
        if b.getName() == name:
            return b
    return None


def read_bytes(program, addr, n):
    """Read `n` bytes as Python bytes.

    Ghidra's Memory.getBytes fills a caller-supplied byte[]; there is no
    overload that returns one. Java bytes are signed, hence the & 0xFF.
    """
    import jpype

    buf = jpype.JArray(jpype.JByte)(n)
    got = program.getMemory().getBytes(addr, buf)
    return bytes((b & 0xFF) for b in buf[:got])
