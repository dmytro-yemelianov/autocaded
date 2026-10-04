# R4 FILLET remembered-radius contract audit

Decision: **enough retained static evidence for AC1.40 DWG persistence at offset 0x1FA; no AC1.2 or historical DXF radius field established.** This is a contract/evidence result, not an implementation or original measured-output approval. Three bounded evidence passes: known help/notes and graph discovery; retained CFG/data-table trace; independent raw-byte, descriptor arithmetic and original sample validation. No guest/oracle/collector/new lifting or evidence mutation.

## Provenance

- ACAD.EXE SHA256 `0080e343930b6fad30b588f05bacb6fb65783d7d560cdf2f116ca09fc70b45b1`.
- ACAD.OVL SHA256 `c2e1ee5b538e53df022dbb8aba6ab65a3946b8a332438bdaf39a3938a43e0da8`.
- Retained `build/recovered-cfg.json` SHA256 `45fa698d17c0a827f542efd3b58f17e88e078c5fdeee62020874579139c1573c`.
- Retained `build/analysis-map.json` SHA256 `14107722dca7594fd91d3da864ec7ee74b36d234cf30b99e79e81311ba6a2053`.

Graph discovery returned no FILLET symbols, so scoped retained JSON/raw-byte reads were used. Independently matched all **1,960 instruction byte strings** in OVL07:0168/e3d, OVL01:1cd4/24de and EXE:6701/680d/691f against their original file offsets: zero mismatches. This establishes retained byte provenance, not execution of any path.

## Recovered facts

1. Retained `acad.hlp` lines 283–296 explicitly specify R to set radius, drawing-remembered state, initial zero, and zero-radius intersection. This is stronger than a session-only preference.
2. OVL07 data maps file base 118528 to DS offset 2. DS:0011 contains `Select two lines or R: `; DS:008f contains `Enter fillet radius: `. Main routine OVL07_CODE:0168 compares selector character 0x52 (R) at 018e–019f and calls 0e3d on the equal branch at 01a6 (`e8940c`). Routine 0e3d prints the radius prompt, reads an eight-byte local at BP-8, checks the input/error state, and on success copies that local to **DS:48fe** at 0e7e–0e88 (destination `8d3efe48`, floating copy helper f8b9). Its zero comparison constant DS:0536 is independently decoded as eight zero bytes.
3. The same FILLET routine loads DS:48fe repeatedly during geometry construction (077b, 09b2, 0ae4, 0b33, 0bce, 0c1b, 0dfb). At 09b2–09bd it copies that value into the generated geometry radius area DS:5383. Thus the stored header state is the FILLET radius used for following operations, not an unrelated last-input scratch field.
4. Resident DS file base is `0x200 + (0x1e25-0x1010)*16 = 0xe350`. Header descriptor table DS:4906 has index 42 raw bytes **`fe 48 08 00`**, i.e. address48fe, size8. Independently summing the 12-byte signature plus sizes for indices1..41 yields **file offset0x1fa**, and adding this field yields0x202. EXE_CODE:680d writes signature `AC1.40` and descriptor indices1 through42 verbatim, then checks final position0x202. The field is an ordinary little-endian floating double, as its eight-byte runtime operations and descriptor agree. Resident initial bytes at48fe are all zero.
5. EXE:691f's raw version table DS:49b2 binds `AC1.2` to final field address48d4 and header end1d8; `AC1.40` binds48fe and202. EXE:6701 reads the descriptor sequence and stops at the selected final address. The radius is **absent from AC1.2's header**; do not write at1fa into an AC1.2 entity stream. The legacy MC0.0 entry is outside native scope.
6. Retained historical DXF reader/writer routines OVL01:1cd4/24de expose EXTENTS, LIMITS, BASE, DWGVIEW, DIMARROW, MODERES, MODEGRID, MODEORTHO, MODEFILL, TXTSIZE, TRACEWID, LAYER and LAYERC. The header writer reaches its layer table via literal DS:c4c at26db. No FILLET/radius token or DS:48fe field mapping exists in this audited header path. This is **not** permission to invent a modern FILLET variable/token. Exact missing fact for DXF support: a retained recognized header token and corresponding reader/writer mapping, or a separately authorized future original measurement establishing a representation.

## Sample and parity boundary

All 24 original `corpus/Samples` DWG/BAK headers were inventoried:16 AC1.2,8 AC1.40. All eight AC1.40 samples have zero at1fa..201. This corroborates the default/layout but does not measure a nonzero original FILLET save/reopen. No claim is made that every other retained test file was surveyed; malformed test fixtures were excluded from the original-sample census. Static descriptor/state evidence is sufficient for a bounded implementation, while original nonzero export/UI parity remains unmeasured.

## Proposed native implementation and acceptance

Add semantic nonnegative finite drawing radius with default0, decode AC1.40 f64 at1fa, and explicitly overwrite that slot when encoding AC1.40 even when preserving raw header bytes. Default AC1.2/DXF imports to0 because no field is available; this defaulting choice is native policy, not a claim about original cross-document session reset internals. For nonzero radius, checked AC1.2 and historical DXF serialization must refuse before staging unless a separately approved explicit lossy-export policy is introduced. Do not silently clear state, invent bytes/tokens or implicitly upgrade the attached document revision. Zero remains representable by omission in those formats.

Required checks: exact synthetic nonzero AC1.40 bytes and decode/reencode (e.g.2.5 => `0000000000000440` at1fa); original zero sample behavior held; negative/NaN/infinite input or persisted values fail under the declared native validation; FILLET R changes drawing state with correct undo/dirty behavior and following FILLET uses it; zero intersection path creates no spurious ARC; R errors/cancel and unsupported SAVE/END/WBLOCK/API preserve drawing, destination, attachment and baseline. Supported AC1.40 reopen restores radius; unsupported formats cannot lose nonzero state. Keep plain FILLET geometry choices and view changes independently scoped.

Audit complete within three evidence passes. R4 required source review and final exact-patch review remain pending V1/B4 candidates; this is not omnibus task completion.
