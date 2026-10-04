# L3 exact signed-layer candidate review — pass 1

Verdict: **PASS scoped source review; no blocking correctness finding.** Exact patch `RUN/l3.patch` SHA256 `dcb0acb4b6fc62d19e885cc1b23fe7778add4ae4ba32ded7c22fd4f952058722`, byte-identical to staged binary diff in `RUN/off-persistence`. Baseline `cb2197ae9a275c7655530bf9dcd8cdc4395fd7c7`;11 staged files, no unstaged changes. This approves the reviewed bounded signed-layer code, not unrestricted historical layer parity or subsequent B3 integration.

## Source assessment

DWG `header.rs` interprets each table word as signed, uses checked_neg before range acceptance, accepts negative magnitudes1..127 only on slots1..127, and preserves the positive magnitude plus semantic OFF set. Invalid high magnitudes and i16::MIN fail without abs overflow. The existing positive0..254 color range and positive255 unused sentinel remain unchanged. Metadata passthrough remains populated from original bytes, while writers rewrite semantic table state explicitly.

DWG `write.rs` validates the sparse layer table and every OFF slot before header output. OFF must reference a defined slot1..127/color1..127. It writes negative magnitude as the existing little-endian16-bit word; it does not OR a bit, truncate a negative magnitude, clear the OFF set or mutate the model. Both revision adapters already route through the same checked encoder. The exact0xc8 table location is unchanged.

Historical DXF `parse.rs` reads signed decimal i16 values and applies the same bounded negative policy. Each later LAYERC table replaces its corresponding slot, removing previous OFF status for positive/sentinel values and removing a prior defined slot for255. Invalid parse paths do not return a partially populated drawing. `try_write` checks sparse indices/reserved colors plus OFF membership/range before building the string. The legacy `write` adapter remains an explicit checked-result panic on unsupported state, not a silent lossy serializer. Production lifecycle/API paths continue using checked writers.

The root-accepted unsupported edges remain explicit: OFF layer0, OFF color0, colors128..254 while OFF, absent/out-of-range OFF slots, invalid negative sentinels/magnitudes. Positive high colors and zero retain existing native semantics. Selecting an OFF current layer continues the declared Rust policy of keeping it OFF; codec load/save does not adopt historical command auto-ON. No entity/repeat grammar or renderer code is changed in this candidate.

## Validation independently assessed

Read the new DWG/DXF assertions and modified command/app lifecycle tests. They check exact negative bytes/text at colors1/7/64/127, positive254 and unused255 distinction, reencode stability, every positive color0..254, invalid negative magnitudes including minimum i16, all public writer routes and legacy DXF failure, repeated LAYERC replacement, and exact positive header-table preservation for24 retained DWG/BAK files. That corpus test intentionally uses header-only reencoding and does not claim unrelated entity-stream roundtrip parity; corpus files are present in this workspace.

Lifecycle assertions span AC1.2, AC1.40 and DXF: supported END/SAVE/API save/WBLOCK/export, reopened layer tables/current layer/OFF set, state/picking/frame agreement, baseline updates and undo after save. Color-zero failure cases check existing destination bytes, attachment/format/dirty/model state, API pending prompt/input, no staging files, and ON/UNDO recovery. Assertions are meaningful for the changed contract rather than merely checking worker log exit status.

No tests/builds/original runtime were rerun by this read-only reviewer. Worker reports414 affected tests and Clippy; root owns independent integrated execution and actual GUI. In particular, the GUI harness's prior blanket OFF-refusal section must change to supported persistence plus an unsupported color-zero case, and B3's metadata/erasure changes still need their own combined review. No new issue is being hidden by treating those future checks as already passed.

## Evidence/claim limits

Documentation correctly distinguishes retained static NEG/table evidence from measured original OFF fixtures: the24 retained headers contain no negative layer words. AC1.2 support follows the retained1.4 reader's shared field table, not a separately measured1.2 producer. Native edge refusals and current-layer policy remain labeled. No new guest/oracle/translated execution, tolerance change, fixture mutation or positive-value normalization is introduced.

AGENT_REVIEW task:R3-L3 verdict:pass findings:no blocking scoped source findings evidence: exact SHA256 and staged identity above; codecs, tests and lifecycle assertions independently read next: root integrated tests/updated GUI; combined B3/render changes require exact review notes:this file.
