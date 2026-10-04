# acad-dwg test fixtures

Original AutoCAD 1.4 output, generated with the in-tree emulator
(`acad_oracle::in_tree`) from the read-only
`corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img`. They are regenerated and
compared byte for byte by `crates/acad-oracle/tests/task6_placeholders.rs`
(`the_committed_fixtures_are_the_originals_own_bytes`). Contract:
docs/native-group-persistence.md, "Task 6 placeholder records".

| File | Bytes | SHA-256 | How the original made it |
| --- | --- | --- | --- |
| `task6-line.dwg` | 1024 | `ab48ff8cf9db2da887ab2c5daa25cf71ea37e6af0ffc86bec134b6fee9706f76` | Main Menu task 6 (Load DXF) into a new `D3.DWG` from the original's own task 5 DXF of a one-LINE (0,0)-(5,5) drawing. 13 records: 12 zero-filled erased placeholders (-1..-13 without -5), then the LINE. |
| `task6-line-ended.dwg` | 640 | `82cdc32327b1c100f944774249fd3e189c2e3378bfeaed726bb596be65e28da2` | `task6-line.dwg` opened with task 2 and ENDed unchanged. 1 record: the LINE. |
| `task6-dimarrow.dwg` | 1024 | `fe07b677a7af2b0182766a04b02e13d5642ef40f8c60f4ef256c5e44e602ed2c` | Task 6 into a new `D3.DWG` from the native `acad_dxf::try_write` of the same drawing with `dim_arrow = Some(0.18)`. 14 records: placeholders -1..-13 (the erased REPEAT start -5 directly before the erased ENDREP -6), then the LINE. |

All are AC1.40 (the original only writes that revision); the acad-dwg tests
move the same record bytes behind an AC1.2 header for the other revision.
