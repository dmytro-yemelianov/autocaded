#!/usr/bin/env python3
"""Generate demo/AUTOCADED.SHP, AutoCADED's own monoline stroke font.

An original design for the public demo, so the browser build ships no
Autodesk font. Glyphs live on an 8 x 12 grid: cap height 12, x-height 8,
descender 4, monospaced with a 12-unit advance. Strokes are polylines
("x,y x,y ..."), separated by "|", written in the SHP source format the
engine reads (crates/acad-render/src/shp.rs): pen up 2 / pen down 1,
code 8 for one displacement and code 9 for a (0,0)-terminated list.

    python3 tools/demo/gen_font.py
"""
from pathlib import Path

ADVANCE = 12
NAME = "AutoCADED Mono"

O_UPPER = "2,0 0,2 0,10 2,12 6,12 8,10 8,2 6,0 2,0"
P_BOWL = "0,0 0,12 6,12 8,10 8,8 6,6 0,6"

GLYPHS = {
    " ": "",
    "!": "4,12 4,4 | 4,1 4,0",
    '"': "2,12 2,9 | 6,12 6,9",
    "#": "2,0 3,12 | 5,0 6,12 | 0,4 8,4 | 0,8 8,8",
    "$": "8,10 6,12 2,12 0,10 0,8 2,6 6,6 8,4 8,2 6,0 2,0 0,2 | 4,13 4,-1",
    "%": "0,0 8,12 | 1,12 0,11 1,10 2,11 1,12 | 7,2 6,1 7,0 8,1 7,2",
    "&": "8,0 1,9 1,11 2,12 4,12 5,11 5,9 0,4 0,2 2,0 5,0 8,4",
    "'": "4,12 4,9",
    "(": "6,13 4,11 3,8 3,4 4,1 6,-1",
    ")": "2,13 4,11 5,8 5,4 4,1 2,-1",
    "*": "4,10 4,2 | 0,8 8,4 | 0,4 8,8",
    "+": "4,10 4,2 | 0,6 8,6",
    ",": "4,1 4,0 3,-2",
    "-": "1,6 7,6",
    ".": "4,1 4,0",
    "/": "0,0 8,12",
    "0": O_UPPER + " | 0,2 8,10",
    "1": "2,10 4,12 4,0 | 2,0 6,0",
    "2": "0,10 2,12 6,12 8,10 8,8 0,0 8,0",
    "3": "0,10 2,12 6,12 8,10 8,8 6,6 3,6 | 6,6 8,4 8,2 6,0 2,0 0,2",
    "4": "6,0 6,12 0,4 8,4",
    "5": "8,12 0,12 0,7 6,7 8,5 8,2 6,0 2,0 0,2",
    "6": "7,12 3,12 0,9 0,2 2,0 6,0 8,2 8,5 6,7 2,7 0,5",
    "7": "0,12 8,12 3,0",
    "8": "2,6 0,8 0,10 2,12 6,12 8,10 8,8 6,6 2,6 0,4 0,2 2,0 6,0 8,2 8,4 6,6",
    "9": "8,7 6,5 2,5 0,7 0,10 2,12 6,12 8,10 8,3 5,0 1,0",
    ":": "4,8 4,7 | 4,1 4,0",
    ";": "4,8 4,7 | 4,1 4,0 3,-2",
    "<": "8,11 0,6 8,1",
    "=": "0,8 8,8 | 0,4 8,4",
    ">": "0,11 8,6 0,1",
    "?": "0,10 2,12 6,12 8,10 8,8 4,5 4,3 | 4,1 4,0",
    "@": "6,4 3,4 2,5 2,7 3,8 6,8 6,3 7,2 8,3 8,10 6,12 2,12 0,10 0,2 2,0 7,0",
    "A": "0,0 4,12 8,0 | 1,3 7,3",
    "B": "0,0 0,12 6,12 8,10 8,8 6,6 0,6 | 6,6 8,4 8,2 6,0 0,0",
    "C": "8,10 6,12 2,12 0,10 0,2 2,0 6,0 8,2",
    "D": "0,0 0,12 5,12 8,9 8,3 5,0 0,0",
    "E": "8,12 0,12 0,0 8,0 | 0,6 6,6",
    "F": "8,12 0,12 0,0 | 0,6 6,6",
    "G": "8,10 6,12 2,12 0,10 0,2 2,0 6,0 8,2 8,5 4,5",
    "H": "0,0 0,12 | 8,0 8,12 | 0,6 8,6",
    "I": "2,12 6,12 | 4,12 4,0 | 2,0 6,0",
    "J": "8,12 8,2 6,0 2,0 0,2",
    "K": "0,0 0,12 | 8,12 0,4 | 3,7 8,0",
    "L": "0,12 0,0 8,0",
    "M": "0,0 0,12 4,6 8,12 8,0",
    "N": "0,0 0,12 8,0 8,12",
    "O": O_UPPER,
    "P": P_BOWL,
    "Q": O_UPPER + " | 5,3 8,0",
    "R": P_BOWL + " | 4,6 8,0",
    "S": "8,10 6,12 2,12 0,10 0,8 2,6 6,6 8,4 8,2 6,0 2,0 0,2",
    "T": "0,12 8,12 | 4,12 4,0",
    "U": "0,12 0,2 2,0 6,0 8,2 8,12",
    "V": "0,12 4,0 8,12",
    "W": "0,12 2,0 4,8 6,0 8,12",
    "X": "0,0 8,12 | 0,12 8,0",
    "Y": "0,12 4,6 8,12 | 4,6 4,0",
    "Z": "0,12 8,12 0,0 8,0",
    "[": "6,13 3,13 3,-1 6,-1",
    "\\": "0,12 8,0",
    "]": "2,13 5,13 5,-1 2,-1",
    "^": "1,9 4,12 7,9",
    "_": "0,-2 8,-2",
    "`": "3,12 5,10",
    "a": "1,8 6,8 8,6 8,0 | 8,4 2,4 0,2 2,0 6,0 8,2",
    "b": "0,12 0,0 | 0,6 2,8 6,8 8,6 8,2 6,0 2,0 0,2",
    "c": "8,6 6,8 2,8 0,6 0,2 2,0 6,0 8,2",
    "d": "8,12 8,0 | 8,6 6,8 2,8 0,6 0,2 2,0 6,0 8,2",
    "e": "0,4 8,4 8,6 6,8 2,8 0,6 0,2 2,0 7,0",
    "f": "7,12 5,12 3,10 3,0 | 0,8 7,8",
    "g": "8,8 8,-2 6,-4 1,-4 | 8,6 6,8 2,8 0,6 0,2 2,0 6,0 8,2",
    "h": "0,12 0,0 | 0,6 2,8 6,8 8,6 8,0",
    "i": "4,8 4,0 | 4,11 4,12",
    "j": "6,8 6,-2 4,-4 1,-4 | 6,11 6,12",
    "k": "0,12 0,0 | 7,8 0,1 | 3,4 7,0",
    "l": "4,12 4,2 6,0",
    "m": "0,8 0,0 | 0,6 2,8 3,8 4,6 4,0 | 4,6 6,8 7,8 8,6 8,0",
    "n": "0,8 0,0 | 0,6 2,8 6,8 8,6 8,0",
    "o": "2,0 0,2 0,6 2,8 6,8 8,6 8,2 6,0 2,0",
    "p": "0,8 0,-4 | 0,6 2,8 6,8 8,6 8,2 6,0 2,0 0,2",
    "q": "8,8 8,-4 | 8,6 6,8 2,8 0,6 0,2 2,0 6,0 8,2",
    "r": "0,8 0,0 | 0,5 3,8 7,8",
    "s": "8,7 6,8 2,8 0,7 0,5 2,4 6,4 8,3 8,1 6,0 2,0 0,1",
    "t": "3,12 3,2 5,0 7,0 | 0,8 7,8",
    "u": "0,8 0,2 2,0 6,0 8,2 | 8,8 8,0",
    "v": "0,8 4,0 8,8",
    "w": "0,8 2,0 4,6 6,0 8,8",
    "x": "0,0 8,8 | 0,8 8,0",
    "y": "0,8 4,0 | 8,8 2,-4",
    "z": "0,8 8,8 0,0 8,0",
    "{": "6,13 4,12 4,7 2,6 4,5 4,0 6,-1",
    "|": "4,13 4,-1",
    "}": "2,13 4,12 4,7 6,6 4,5 4,0 2,-1",
    "~": "0,6 2,8 4,6 6,4 8,6",
}


def strokes(spec):
    for stroke in filter(None, (part.strip() for part in spec.split("|"))):
        yield [tuple(int(v) for v in point.split(",")) for point in stroke.split()]


def program(spec):
    """SHP bytes for one glyph, ending with the pen at the advance point."""
    code, x, y = [], 0, 0
    for points in strokes(spec):
        (sx, sy), rest = points[0], points[1:]
        code += [2, 8, sx - x, sy - y, 1]
        x, y = sx, sy
        code.append(9)
        for px, py in rest:
            code += [px - x, py - y]
            x, y = px, py
        code += [0, 0]
    code += [2, 8, ADVANCE - x, -y, 0]
    return code


def render(code):
    out, i = [], 0
    while i < len(code):
        op = code[i]
        if op == 8:
            out.append(f"8,({code[i + 1]},{code[i + 2]})")
            i += 3
        elif op == 9:
            pairs, i = [], i + 1
            while True:
                pairs.append(f"({code[i]},{code[i + 1]})")
                i += 2
                if pairs[-1] == "(0,0)":
                    break
            out.append("9," + ",".join(pairs))
        else:
            out.append(str(op))
            i += 1
    return ",".join(out)


def main():
    assert set(GLYPHS) == {chr(c) for c in range(32, 127)}, "every printable ASCII glyph"
    lines = [f"*0,4,{NAME}", "12,4,0,0"]
    for char in sorted(GLYPHS, key=ord):
        code = program(GLYPHS[char])
        assert all(-127 <= v <= 127 for v in code)
        lines += [f"*{ord(char)},{len(code)},{'spc' if char == ' ' else 'c' + str(ord(char))}", render(code)]
    target = Path(__file__).resolve().parents[2] / "demo" / "AUTOCADED.SHP"
    target.parent.mkdir(exist_ok=True)
    target.write_bytes(("\r\n".join(lines) + "\r\n").encode("ascii"))
    print(f"wrote {target} ({len(GLYPHS)} glyphs)")


if __name__ == "__main__":
    main()
