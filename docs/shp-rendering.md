# SHP font and shape rendering

The native app and `render-png` now draw TEXT and SHAPE entities from the
original libraries. No font data is embedded in the binary or committed here.

```sh
cargo run -p acad-app -- corpus/Samples/DISC.BAK
cargo run -p acad-render --example render-png -- corpus/Samples/DISC.BAK /tmp/disc.png
```

Additional arguments are font directories, searched before the defaults.
For drawings in the extracted corpus, System supplies the startup TXT font;
Samples supplies other fonts and shape libraries. System/TXT is Roman Simplex
with cap height 21; Samples/TXT is the older Standard font with cap height 6.
Explicit A:/B: aliases distinguish those libraries. Other DOS paths are
relocated by basename within the supplied directories, not opened directly.

## Implemented behavior

The parser handles definition headers, declared byte counts, decimal and
leading-zero hexadecimal bytes, signed displacements, comments, parentheses,
and DOS EOF (excluding cluster slack). Duplicate IDs and malformed counts
are errors.

Instructions 0–9 cover all seven supplied files: termination, pen controls,
scaling, saved positions, subshape calls, and individual/repeated x/y moves.
Packed vectors preserve their major-axis length. Subshapes share pen, scale,
position, and stack state. Text uses its font's cap height and the program's
pen-up advance, then entity and enclosing block transforms. Loading a shape
library retains the current text font; loading a font changes subsequent text.

Missing libraries, glyphs, subshapes, blocks, unsupported instructions, stack
errors and recursion limits are reported by `flatten_with_libraries`. The app
prints each diagnostic once. The older `flatten` remains a geometry-only API;
callers wanting fonts and diagnostics must supply libraries explicitly.

The interpreter uses limits of 32 nested subshapes/stack entries and 100,000
instructions per text or shape. These are implementation bounds, not recovered
AutoCAD 1.4 limits. Opcodes above 9 are rejected: none of the supplied files
uses them. Compiled SHX, Big Fonts, Unicode fonts, and vertical-text instructions
are not implemented.

## Evidence and tests

- All seven original libraries parse. Every printable font glyph and every
  ES/PC shape executes; PC exercises nested subshape calls.
- All 21 drawings render without missing-library or missing-glyph diagnostics.
- The Roman A test derives its two legs, crossbar and 22-unit advance directly
  from System/TXT.SHP, then checks two adjacent characters.
- Synthetic geometry checks pen-up movement, stack restoration, nested scaling,
  all 16 vector directions, ordered font changes, and text rotated inside an
  unequally scaled, rotated block.
- `acad-oracle/tests/font_render.rs` asks the original to draw a known rectangle,
  Roman `AA`, and the ES `RES` shape. The rectangle calibrates the original's
  unequal CGA pixel scales. Separate text and resistor regions compare native
  stroke samples against lit original pixels in both directions, using a
  1.5-pixel tolerance and a 98% minimum. The initial run scored 100% both ways
  for both regions; neither the rectangle nor UI pixels count toward these checks.
- Deliberately multiplying rendered text height by 0.75 made that test fail:
  native-to-original coverage fell to 32.94%, and original-to-native coverage
  to 27.78%. The correct renderer was restored byte for byte afterward.
- Capture waits for the BIOS keyboard queue to drain and for stable CGA memory.
  A fixed delay previously captured the editor before a queued SHAPE completed.

The original image test establishes this small rendering case, not pixel-exact
reproduction of every font or drawing. Other fonts are checked from their
instruction streams and inspected in the native output. LOAD scoping inside
block bodies still needs a dedicated original-command probe; the renderer
currently carries library state through an inserted block in traversal order.

Modern Autodesk references corroborate the vector and instruction meanings:
[vector codes](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Customization/files/GUID-0A8E12A1-F4AB-44AD-8A9B-2140E0D5FD23.htm)
and [special codes](https://help.autodesk.com/cloudhelp/2022/ENU/AutoCAD-Customization/files/GUID-06832147-16BE-4A66-A6D0-3ADF98DC8228.htm).
Those later documents alone are not evidence of 1983 compatibility; the
original library files and CGA comparison provide that check here.
