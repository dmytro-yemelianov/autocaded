# Default TXT horizontal metric coverage

`src/txt_metrics.rs` records X advances and ink X bounds for the 94 printable
characters defined by the retained System/TXT.SHP. Space has advance and no ink;
ASCII 96 (backtick) is absent. The font SHA-256 is
`cd5264bb4cd240b61b102e62d56c4281aeb76b9259f6e58f644e28a34aa9166c` and cap height is 21.
The table contains derived scalar metrics; no font program is copied into Rust.

Reproduce the data and its composition audit with:

```sh
cargo run -p acad-render --example txt-metrics -- corpus/System/TXT.SHP
```

The utility interprets every defined printable glyph with the existing SHP
renderer, then compares every one of the 8,836 ordered pairs with independent
glyph composition. It checks complete ordered strokes and both advance axes,
so matching bounding widths alone cannot conceal changed pen/scale/stack state
for a tested pair. Undefined characters are explicit empty entries. The emitted
data needs only X advances for the horizontal bound calculation. These are
pairwise composition checks, not a general proof about all fonts or stateful SHP
programs. The application and portable tests do not need the ignored font file.

DIM unions each glyph's translated ink bounds, adding advances between glyphs.
Leading/trailing spaces affect pen position but not ink width. The final glyph's
advance is not included in the right ink edge. For example, MMMM is 88 font units
wide and mmmm is 112; their former numeric fallback gave both 74. Other new
symbols include the plus sign used in positive scientific exponents.

Unit tests pin these widths, spacing, earlier numeric/native labels and the
unchanged fallback for absent characters. The editor test covers a MMMM label
whose wider ink changes arrows from internal to external, every text field,
DWG save/reopen and UNDO. The 32 original scripts (29 at the time of this
metric work, plus DAR050/DAR200/PTEXT50) compare all 344 ordered native
primitives at absolute 1e-10 geometry tolerance.

The new glyph widths have renderer evidence, not additional original DIM
placement exports. Vertical bounds, fonts other than TXT, unsupported characters
and the vertical-line arrow-fit comparator retain their documented limitations;
large-arrow text placement is measured in [native DIM](../../../../../docs/native-dim.md).

Static inspection of the retained recovered code confirms the original DIM
measure helper (OVL04:14B0) calls the font bounds helper (EXE:8DDF), then subtracts
the accumulated X bounds. The saved A=.5/A=2 origins do not follow the current
Rust stroke-centering policy. Inspection did not establish the missing general
placement rule, so those cases remain excluded from passing native geometry
coverage. No coordinate lookup, tolerance change or guest execution was used.
