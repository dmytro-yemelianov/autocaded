# B3 independent physical-grammar and model contract review

Verdict: **PASS for the corrected independent-marker grammar; conditional contract-ready for bounded B3a implementation. B3b group ERASE/OOPS sign scope remains unresolved.** No implementation reviewed or approved. This review does not adopt the newly added clean-room specification as authority or expand scope.

Read-only sources: worker proposal `RUN/repeat-codec/docs/native-group-persistence.md`; retained `build/recovered-cfg.json` and `build/analysis-map.json`; original EXE/OVL and FLOOR/BLIVET/ADDER; current Rust model/codec for integration implications. Graph discovery found existing Rust symbols but not retained machine-code routines, so scoped static reads supplied those routines. Three bounded aspects were reviewed: physical grammar; independent bytes/corpus walk; model/visibility implications. No original/translated runtime, collector or new oracle run; no code/evidence edits.

## Independent evidence confirmation

Original primary hashes are unchanged from L2: ACAD.EXE `0080e343930b6fad30b588f05bacb6fb65783d7d560cdf2f116ca09fc70b45b1`, ACAD.OVL `c2e1ee5b538e53df022dbb8aba6ab65a3946b8a332438bdaf39a3938a43e0da8`. DS file base independently computed from MZ/header and mapped segments is0xe350. Compared650 instructions in EXE:3770,3985 and OVL02:c7d1,9ce0,e4cd against original file offsets: zero mismatches.

- EXE:3770 reads signed type DS:42be and independent layer DS:42c0, chooses descriptor using abs(type), then loops six-byte descriptors. DS:4426+4*5 contains name3883=`REPEAT start`, descriptor4360. Original descriptor words are(0,0,0); its body-field loop terminates immediately. **Type5 is an independent four-byte physical record.**
- Type6 points to4366 with selectors(4,1),(4,2),(2,2),(2,3), then zero. The reader branches read two bytes for selector4 and eight for selector2. Thus ENDREP has its own four-byte header plus two u16 dimensions/two f64 spacings:24bytes total.
- EXE:3985 chooses descriptor using absolute type, but writes the original signed DS:42be and layer before fields. Negative type5 therefore has the same four-byte physical size. This proves layout, not which sign pattern represents a whole erased logical group.
- OVL02:c7d1 increments a local depth on positive type5 atc874/c881 and decrements on positive type6 atc88c/c899; at ENDBLK it tests depth zero. This independently corroborates structural marker balance. It neither proves historical editing/selection behavior for arbitrary nested repeats nor establishes a historical depth/resource limit.

I independently walked the three files using raw original descriptor field sizes (point16,scalar8,word2,length-prefixed string2+length), without calling the existing Rust parser. Counts and exact end positions:

| File | SHA256 | Physical count/header count | Final offset/header end | Marker/erased observations |
|---|---|---|---|---|
| FLOOR.DWG | 618649ecff681fd6e46bfb60b7223fbf02f5626b181d7fb97df02a6462d614dc |213/213|0x204f/0x204f|Starts0x16c9,0x1775; balanced depth1|
| BLIVET.DWG | aa44f9049bb22acd5dd77b65d120510e05fd7363cbb2f94abfb2897a30f09b89 |149/149|0x1a4c/0x1a4c|Starts0x3b4,0x3f4,0x434,0x14a7,0x1880; balanced depth1|
| ADDER.DWG |1882190112f93b683114df1f1d31d0e85769d582a0e82511f5cde94a86177a7d|66/66|0xb50/0xb50|Five negative LINE/two negative INSERT; no REPEAT|

All seven opening markers, their immediately following child headers and closing markers have layer1. These examples cannot distinguish marker layer from first-child layer, or establish marker ownership. The original descriptor does distinguish the headers decisively. A start marker followed by an ordinary child gives the same byte sequence formerly described as a fused record. The old “opaque_word” is the child's actual layer; the old count2 is simply two physical records.

## Required corrections and honest model contract

P1 current-code defect established: `crates/acad-dwg/src/entity.rs::read_record_body` uses the marker's layer for the first child and ignores the real child layer. `crates/acad-dwg/src/write.rs::encode_repeat` replaces the child's layer with1 while deriving marker layers from it. The apparent flat layer1 roundtrip agreement masked this. Correct reader/writer must count and emit each header independently; no special first-child semantics.

Represent at least **start marker layer** and **end marker layer** separately from `Repeat.entities` and their ordinary per-entity layer wrappers. Keep this metadata in the shared Repeat representation so Item::Repeat and Entity::Repeat preserve the same information through COPY, ARRAY, MOVE, SCALE, ROTATE, BLOCK/WBLOCK, undo and codec roundtrip. Both marker layers can differ from each other and from every child; this audit does not license collapsing them. Existing newly created bare groups may choose explicit documented defaults (e.g.1 for each marker), which are native construction policy. Marker fields read from files must not be recomputed from children. Validate supported layer ranges without truncation. API/source-literal adjustments required by a new model field are implementation work, not a reason to discard metadata.

**Runtime OnLayer ownership is separate.** The verified L1 native policy gives bare repeats no owner gate and explicit OnLayer(REPEAT) a group gate plus child gates. A historical file always has a start and an end marker layer; no inspected bit/sentinel says whether a native optional owner wrapper existed. Accordingly:

- Decode preserved marker metadata into a bare group under the existing native policy. Do not automatically wrap a group merely because the file marker has a layer. This preserves bare-group behavior across native save/open and leaves child layers intact.
- Preserve explicit wrappers in memory. Do not strip them, push them into every child, equate them with the first child, or convert marker metadata into an owner merely to make tests pass.
- **Until an ownership mapping is proved or explicitly specified as a new native format, checked-refuse serialization of explicit owner-wrapped REPEAT states that cannot be represented losslessly.** A conservative implementation can refuse every explicit OnLayer(REPEAT), including nested ones. A narrower “redundant wrapper” exception needs a real equivalence argument for future layer edits and all descendants/resources, not only a screenshot with all layers ON.
- Thus B3a can support bare flat/nested streams and preserve their actual leaf layers; it does not complete persistence of every currently constructible native group. Keep the explicit-wrapper restriction visible in the ledger and API errors. Safe refusal is a safety boundary, not functional completion.

This is an honest bounded native interpretation of the physical grammar, not proof that original AutoCAD ignores marker layers for visibility. The limited static visibility follow-up finds geometry paths EXE:192d (1e57–1e76) and4e01 (4e3a–4e59) forwarding current record layer into7154/72eb, with special treatment of layer127 through DS:4260. It does not establish a REPEAT owner's gate or the full inherited-layer semantics. INSERT/reference context exists at43db. Do not generalize these snippets into repeat ownership, use127 as a new sentinel, or revise L1 layer policy without a separate supported contract. Original mixed-layer repeat visibility remains unmeasured/unresolved.

## Physical records versus logical assembly

Make physical decoding return start/end markers with independent layer, offset and sign, each count1. Keep entity_count/end checks independent of grouping. A bounded stack can then construct nested live groups; enforce matching ends, explicit depth/work caps, truncated-record checks and block-root boundaries. Document caps as native safety policy. Nested block definitions are already separately modeled; do not accidentally attach an open outer repeat to a newly opened block or consume its closer there. Named block contents can be independent repeat roots. Shared aliases/convenience APIs must retain their documented behavior instead of changing silently.

Erased ordinary records inside groups also require an explicit storage/error decision: current Vec<Entity> lacks the top-level Item::Erased tag. Do not silently drop them. Preserve a per-record erased state in an appropriate model form, or reject the unsupported logical combination with a named error. The physical layout may still be decoded accurately without inventing group semantics. Negative structural markers must not become invisible Skip records or be silently normalized positive.

Historical DXF also carries independent REPEAT/ENDREP keyword records, but this audit's raw descriptor proof is specifically DWG. Any shared logical-assembler reuse must independently preserve DXF suffix layers and use its existing proven fields. Do not assume signed DWG structural-type semantics transfer to DXF merely because L2's layer-color words have signed DXF support.

## B3b unresolved question, stated precisely

OVL02:9ce0 negates DS:42be at9d38 for each selected address and writes it; e4cd reverses the sign ate52c for each retained restoration-list address. Physical bodies are preserved by3985. The unresolved static question is: **when one REPEAT owner is selected, which physical record addresses does the selection-cache producer put into the iterator/list consumed by ERASE and later OOPS—start only, all descendants/closer, or another set, including nested/block boundaries?** DS:58a6 identifier and58aa/58a8 address returned by984a are leads, not a proven answer. The rendering/iterator treatment of negative start/end markers and remaining positive children must agree with any proposed group-sign mapping.

After the worker's three bounded passes this remains an explicit compatibility gap. Neither negating the opening marker nor negating every enclosed record is established whole-group parity. No persisted editor undo-stack/OOPS-after-open behavior is established. Root may separately authorize a labeled native serialization policy, but it must preserve semantics, be reviewable, and must not be called recovered original behavior. Existing unsupported guards remain appropriate until the chosen contract is complete; adding DXF refusal prevents silent loss but does not finish B3b.

## Acceptance cases for exact implementation review

- Read a four-byte start followed by a first child with a different layer, multiple further child layers and a separately different closing-marker layer; write/open preserves all fields and visible native geometry under each relevant OFF set.
- Both DWG revisions: exact physical record counts and boundaries; independent start/end bytes; ordinary first-child headers unchanged. Re-run held FLOOR/BLIVET/ADDER checks without tolerance changes or fixture rewriting.
- Bare flat/nested groups, first child nested group, metadata-only first child, and groups inside independently referenced block definitions. Preserve LOAD order and transitive block closure. Clearly classify empty-group support/rejection as native policy unless retained original evidence exists.
- Explicit owner wrappers with mixed-layer/nested descendants: preserve current in-memory visibility and fail before output/staging if persistence is not representable. Check all serializers, SAVE/END/WBLOCK and API routes, not only the DWG helper.
- Malformed/cross-boundary/nesting-limit inputs, negative markers and erased members: no partial model, skipped state, count bypass or overwritten destination. Erased ordinary top-level ADDER records retain prior supported behavior.
- Canonical IDs, selection/LIST, transformations, hatch/array dispatch, bounds/picking/highlights keep one stored group owner and preserve metadata. Bounded nested parsing does not authorize unbounded rendering or recursive cleanup.

AGENT_REVIEW task:B3-contract verdict:contracts-ready-with-limits findings:P1 fused-first-child model disproven; independent marker metadata required; explicit runtime owner persistence unresolved; group ERASE/OOPS scope unresolved evidence:650 raw instruction matches, original descriptors, three independent exact record walks next: root accepts bounded B3a contract and remaining gaps before code; exact R3 source review after patch notes: this file.
