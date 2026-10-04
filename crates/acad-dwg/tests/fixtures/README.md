# acad-dwg test fixtures

Original AutoCAD 1.4 output, generated with the in-tree emulator
(`acad_oracle::in_tree`) from the read-only
`corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img`. They are regenerated and
compared byte for byte by `crates/acad-oracle/tests/task6_placeholders.rs`
(`the_committed_fixtures_are_the_originals_own_bytes`), and
`crates/acad-oracle/tests/empty_repeat.rs` (same test name) for the
`empty-repeat-*` files. Contract: docs/native-group-persistence.md, "Task 6
placeholder records" and "R6 empty REPEAT groups".

| File | Bytes | SHA-256 | How the original made it |
| --- | --- | --- | --- |
| `task6-line.dwg` | 1024 | `ab48ff8cf9db2da887ab2c5daa25cf71ea37e6af0ffc86bec134b6fee9706f76` | Main Menu task 6 (Load DXF) into a new `D3.DWG` from the original's own task 5 DXF of a one-LINE (0,0)-(5,5) drawing. 13 records: 12 zero-filled erased placeholders (-1..-13 without -5), then the LINE. |
| `task6-line-ended.dwg` | 640 | `82cdc32327b1c100f944774249fd3e189c2e3378bfeaed726bb596be65e28da2` | `task6-line.dwg` opened with task 2 and ENDed unchanged. 1 record: the LINE. |
| `task6-dimarrow.dwg` | 1024 | `fe07b677a7af2b0182766a04b02e13d5642ef40f8c60f4ef256c5e44e602ed2c` | Task 6 into a new `D3.DWG` from the native `acad_dxf::try_write` of the same drawing with `dim_arrow = Some(0.18)`. 14 records: placeholders -1..-13 (the erased REPEAT start -5 directly before the erased ENDREP -6), then the LINE. |
| `empty-repeat-ended.dwg` | 640 | `eeefabf98a0a2835e8b4ab1f1a54ca8d802015a9f4faf0cb735133b7497f4977` | A new `EOALL.DWG`: `REPEAT`, LINE (1,1)-(2,1), LINE (1,3)-(2,3), `ENDREP 2 1 5`, `ERASE W 0,0 10,5`, END; then opened with task 2 as `P2.DWG` and ENDed. 2 records: the positive REPEAT/ENDREP pair alone. |
| `empty-repeat-nested-ended.dwg` | 640 | `b9aed0332d2178563cdff55c1cd247c8c31708f19d36b2704521947f8a733e16` | A new `NESTE.DWG`: `REPEAT`, `REPEAT`, LINE (1,1)-(2,1), `ENDREP 2 1 5`, LINE (1,3)-(2,3), `ENDREP 1 2 5`, `ERASE W -1,-1 20,20`, END; then reopened as `P2.DWG` and ENDed. 4 records: outer start, inner start, inner ENDREP, outer ENDREP. |
| `empty-repeat-ended.dxf` | 896 | `9a82f2ae22ae54ea8b5b866d98315aff8ddf55712097fd2b14d5977d5c78fd37` | Task 5 (Make DXF) of `empty-repeat-ended.dwg` as `P2.DWG`: the header records, then `REPEAT,1`, `ENDREP,1`, `2,1,5.000000,0.000000`, the end marker 0x1A and the original's zero padding to the end of its last sector. |

All DWG files are AC1.40 (the original only writes that revision); the acad-dwg tests
move the same record bytes behind an AC1.2 header for the other revision.
