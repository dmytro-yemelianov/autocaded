# Continuous HATCH definition coverage

The production family constants in `src/hatch_pattern.rs` are transcribed from
`corpus/System/ACAD.PAT`, SHA-256
`8ea70d7a5501ce829ff10d4846b009907a964b4057f1fdfe012df4f7f1c7a4d2`.
They do not require that ignored corpus file at runtime or during tests.
The existing command catalogue is unchanged.

| Pattern | Definition angles | Origins (x,y) | Perpendicular spacing |
|---|---|---|---|
| GRATE | 0°,90° | (0,0),(0,0) | 1/32,1/8 |
| NET3 | 0°,60°,120° | (0,0) for each | 1/8 for each |
| PLAST | 0° for each | (0,0),(0,1/32),(0,1/16) | 1/4 for each |
| PLASTI | 0° for each | PLAST origins, then (0,5/32) | 1/4 for each |
| STEEL | 45° for each | (0,0),(0,1/16) | 1/8 for each |

All rows have zero drift and no dash entries. Each family's infinite lines are
`dot(point,normal) = scale * dot(origin,definition_normal) + n * scale * spacing`.
The command angle rotates direction and origin together around world (0,0).
In particular, STEEL's second origin is a world Y translation of 1/16; its
perpendicular phase is 1/(16*sqrt(2)), not 1/16.

Clipping, holes, endpoint direction, center-out sweep and definition-row order
reuse the existing LINE/NET implementation. The 100,000-stroke budget applies
to the whole pattern, including split strokes. A failure adds no block, INSERT
or undo snapshot. Signed dash/gap rows are covered in
[dash definition coverage](dashed-patterns.md); dots are covered in
[dot coverage](dot-patterns.md). Island styles, `U` and external pattern
files are covered in `docs/native-hatch-styles.md` and
`docs/native-hatch-user.md`; the latter also records the original's start-row
truncation and continuous-row orientation, which the sweep now follows.

`hatch_continuous.rs` independently checks complete unit-square geometry,
distinct shifted-family origins, family/sweep order for axis-aligned patterns,
rotation/scale covariance, annulus endpoints and hole exclusion, DWG save/reopen,
UNDO and aggregate overflow. LINE/NET retained-export regressions remain required.
The five new patterns have no retained native exports. These tests validate the
Rust interpretation of the file definitions, not original output ordering,
floating-point behavior, prompts or full native parity. No new guest was run.
