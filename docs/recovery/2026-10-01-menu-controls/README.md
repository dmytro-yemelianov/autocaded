# Native menu controls recovery — 2026-10-01

Evidence collected from AutoCAD 1.4, QEMU 11.0.1 (`-d nochain`), baseline
`35248a0`. The source System.img SHA-256 is
`7848be12b368cb3f11b28fe285bf28b486a0e909f299c32006be9f790f8931b1`.
Each case booted a separate private `Session::boot_disposable` copy. Only one
guest ran at once. The corpus image was never passed to a writing guest.

The recovery example is an observation collector, not an assertion of desired
behavior. It refuses nonempty output directories and missing prerequisites.
`MENU_CONTROLS_CASES` selects comma-separated IDs from its finite table. Its sole
optional positional argument is the evidence directory. It pauses for an empty
stdin line after the operator inspects each pre-click label, each post-click
prompt, and the final Command prompt before END. EOF or another response aborts.

```
CARGO_TARGET_DIR=target/menu-controls MENU_CONTROLS_CASES=BASEOFF,SCIDLE,OCIDLE,SLINE,CLDONE,CSELECT,GOIDLE,RETURN,SEMICOL cargo run -p acad-oracle --example menu-controls-recovery -- docs/recovery/2026-10-01-menu-controls/pilot
```

Coordinates in the transcript are native 640×200 pixels and corresponding
mouse-device positions. PNGs are 640×400 with doubled Y. Pointer values are
Session's packet-mirrored position, not an independent guest readback. Menu
clicks use column 600 and row centers GO 4, Snap 12, Ortho 20, Cancel 156,
NEXT 164; labels and the resulting highlight/prompt were checked in PNGs.
On pages without GO, request `(600,4)` highlighted the first visible entry
rather than establishing a hit on a blank slot. See the narrowed boundary below.

Each run directory contains `provenance.txt`, `transcript.tsv`, raw 16 KiB CGA
and PNG frames under `captures/`, and native `.dwg`, native `.dxf`, and parsed
Header/Item snapshots under `drawings/`. Top-level `transcript.tsv` uses
bundle-relative artifact paths. `observations.tsv` separates observations from
inferences and unresolved points. `SHA256SUMS` indexes exported evidence. [claim-index.tsv](claim-index.tsv)
groups load-bearing claims and named risks with their review artifacts.
Cleanup actions have their own transcript phase; exports are later actions and
must not be mistaken for the pre-cleanup state. Graphics prompts were read from
PNGs, never inferred from the DOS text screen.

## Findings

The staged probes reproduce the prior fresh-start GO error, then distinguish it
from literal semicolon and from command history. GO behaves like one Return
submission in the tested contexts. A fresh automatic menu with no completed
typed command reports `Unknown command. Type ? for list of commands.` for
both GO and physical Return. After explicitly loading `MENU`, `ACAD`, one GO
repeats MENU and opens `MENU File name:`; a second GO submits blank, returns to
Command, and unloads the panel. Physical Return after MENU also opens that
filename prompt. Cancel at that same filename prompt prints `*Cancel*`,
returns to Command and retains the menu; independent POINT `8,7` then succeeds.
Literal semicolon is buffered visibly as `Command: ;` and
only the subsequent Return reports Unknown command.

GO after a completed POINT reopens `POINT Point:` without drawing a point at
the pointer. At LINE first point or CIRCLE radius, empty GO reports `*Invalid*`
and returns to Command; at LINE next point it ends the unfinished line without
adding a segment. GO at empty ERASE selection ends the selection. More
decisively, GO submits paced unreturned `2,3` at LINE's first-point prompt and
continues the same LINE to `4,5`; it also submits a genuinely picked ERASE
selection. The physical Return selection control prints `1 selected, 1 found.`
and saves the same erased LINE as GO. These buffer cases rule out replacing
the current input with an unconditional empty string. Later Return reopens
LINE even after intervening unknown coordinate submissions; the unknown
submissions did not replace that remembered known command.

Snap and Ortho click independently toggle their exported enabled bits without
changing Snap spacing `0.5`. OFF-to-ON-to-OFF clicks and a single click starting ON have
separate exports. During LINE, the same `To point:` prompt and existing anchor
survive each control. The fractional anchor `2.1,3.15` and typed continuation
`4.25,5.15` save unchanged under either enabled control, establishing that
typed points bypass these constraints in those cases. Snap at CIRCLE radius
also preserves the pending center and scalar prompt; radius `1.25` saves
unchanged under either control. The identical `(250,90)` mouse/device
`(8010,9700)` comparison gives these native saved endpoints from anchor `2,3`:

| Case | Snap / Ortho | Native endpoint (rounded here only for readability) |
|---|---|---|
| M00 | OFF / OFF | `6.1274509803922, 4.17647058823534` |
| M10 | ON / OFF | `6.000000000000041, 4.00000000000004` |
| M01 | OFF / ON | `6.1274509803922, 3.00000000000005` |
| M11 | ON / ON | `6.000000000000041, 3.00000000000005` |

M00 preserves the raw point/crosshair reference. Snap changes the visible
coordinate/crosshair; Ortho preserves the raw coordinate/crosshair while its
rubberband and saved endpoint project horizontally. These grid-aligned anchor
cases alone cannot establish rounding or the Snap/Ortho order.

MASYM keeps four LINEs in one native drawing. With both controls OFF,
`(260,85)` saves raw endpoint `6.37254901960789,4.47058823529416` from typed
anchor `2.1,3.15`. A separate LINE with that anchor entered while Snap OFF,
then both toggled ON, saves endpoint `6.500000000000041,3.15000000000005`.
The same raw reference from anchor `5.1,0.2` and separate both-ON continuation
saves `5.10000000000004,4.50000000000004`. The raw coordinates lie strictly in
the upper halves of their positive `0.5` cells. These witnesses reject floor
rounding and preserve the off-grid locked component, differing from Ortho
followed by global snapping. They support nearest positive rounding on a
zero-aligned `0.5` lattice followed by orthogonal projection for these samples.
Saved mouse coordinates retain small native residues (about `5e-14`); the
snapshots preserve every parsed double, including locked components.
This is an inference from finite witnesses, not a total recovered algorithm;
negative coordinates, exact midpoint/axis ties, other spacing/origin choices,
and ordering boundaries remain unproved. Raw references and menu travel are
kept in the transcript, not reconstructed from the app viewport.

Cancel after a completed LINE segment prints `*Cancel*`, returns to Command,
retains that segment, and permits independent POINT `8,7`. Cancel after a
genuinely picked ERASE selection retains the un-erased LINE; the independent
Return control erases that same pick, so this is not an unverified miss.
Idle Cancel, LINE before first point, and LINE after first point likewise
return to Command and permit independent POINT `8,7`; those exports contain
only the new POINT, with no unfinished LINE entity.

Paced text without Return (`p` at idle, `2` at LINE first point) is visibly
followed by `*Cancel*` and a clean Command prompt. The independent POINT
continuation has no stale prefix and saves only `POINT(8,7)`. The analogous
partial typed selection `1` at ERASE is also cleared; it is distinct from the
genuine mouse pick verified by CSELECT/CSELBASE. Cancel at CIRCLE radius saves
no pending CIRCLE. Page-1/page-2 Cancel retains its current panel page and
permits the same independent POINT continuation.

CREPEAT is a preserved native export with a decode failure, not a passing
snapshot. After `REPEAT`, `POINT`, `4,5`, Cancel returns to Command, but ENDREP
still opens `Number of columns:`. The planned subsequent `REPEAT` is consumed
as that numeric answer and prints `*Invalid*`; it never begins a new REPEAT.
POINT `6,5` then succeeds, and another ENDREP again opens `Number of columns:`.
Separate Ctrl-C cleanup returns to Command before native END/export.
The native DXF explicitly contains `REPEAT,1`, POINT `4,5`, POINT `6,5`, and
no ENDREP. The DWG decoder rejects `REPEAT at offset 0x202 has no ENDREP`;
there is no full parsed Header/Item snapshot for this case. Its enabled bits
and spacing in the table are read from native DXF MODE records, and native
DXF structure is quoted as such. Do not infer cancellation clears the repeat
marker or claim a completed/restarted repeat group from this experiment.

The original collector aborted the `cancel` stage at that decode error, so
the remaining cases resume in the empty `cancel-tail` directory; the original
failure is retained. Native files were copied before decoding. CREPEAT hashes
are explicitly marked postrun, because that version hashed only after parse.
The final collector hashes native files before parsing, records per-case
`parse-error.txt` and `decode-failures.tsv`, continues later selected cases,
and exits nonzero at completion if any native export could not be decoded.
Input/capture/export failures still abort with `failure.txt`.

Requests for page-1/page-2 blank top rows dispatched POINT/LIMITS respectively.
A bounded page-1 discriminator first visibly highlights Cancel, then request
`(600,4)` visibly highlights POINT and dispatches it at active LINE next point
(`POINT *Invalid*`, Command). This excludes a remembered Cancel choice. The
logged device/pointer position is `(19230,18930)`; pointer is mirrored, so the
native clamp versus the pixel inverse outside its tested row range remains
unresolved. Do not treat these as verified blank-slot hits or native no-ops.
The separate top drawing request `(250,4)` has no visible drawing crosshair;
it likewise does not certify the inverse at that boundary. Actual GO label
landings are independently visible on page 0.

## Rust refinement and validation

The accepted implementation in commits `7feb962`, `41c411f`, and `9b2b165`
adds exact-byte Snap/Ortho/Cancel routing, typed pending-state preservation,
Cancel buffer clearing with completed entity and repeat-marker retention, and a
shared physical Return/GO submission path. Return/GO repeats observed MENU,
POINT, and LINE histories, including buffered LINE points and picked ERASE
selections. Empty MENU filename unloads; Cancel retains the panel. Rust regressions
refine these finite fixtures and exercise the actual app route. `menu.rs` owns only
exact parsed Header-tail preservation and its parser regression; the `b";"` value
is corpus metadata, while GO behavior follows the observed Return transitions.

The offline oracle coverage consists of eight `menu_control_*` fixture tests;
Task 4 adds `menu_controls_go_native_fixtures`.
`original_menu_controls_pending_snap_matches_native` is the separate live FSLINE
regression. The `menu_control_` filter runs only the eight fixture cases. The Task
3/4 focused native target had ten tests: nine fixture cases and one actual QEMU
FSLINE execution, with zero skips. Final slice validation and exact test counts
are recorded in the Task 5 report. These are finite Rust refinements, not general
native parity. No desktop/DPI runtime verification was performed.

`submit_mouse_point` remains unchanged. Snap and Ortho toggle exported flags,
while observed typed points bypass those constraints; mouse projection remains
unimplemented. True native blank-slot hit mapping is uncertified. Exact ties,
origin, negative rounding, arbitrary spacing, axis choice, projection order, and
view rules remain unsupported. Partial-buffer toggle parity, general command
history, and broad native constraints are not claimed. M1–M3 custom macro behavior
remains as described in the handover.

## Axis witnesses and remaining geometry boundary

MAXIS saves six LINEs: one raw/control pair for each native endpoint
`(260,60)`, `(260,79)`, `(260,74)`, all from the mouse anchor `(200,100)`.
Each matched anchor is entered with both controls OFF, then Ortho is toggled
ON at the next-point prompt; the transcript preserves that menu travel.
All six starts are exactly the parsed doubles
`(4.901960784313769,3.58823529411769)`. Final limits/base/view match the BASEOFF
snapshot, Snap stays OFF with spacing `0.5`, and Ortho is restored OFF after
each pair. Invariance within the session is supported by the input sequence,
matched native starts/raw references, and final header; no live header peek
was performed between segments.

| Endpoint row | Absolute raw world ΔX / ΔY | Saved axis | World candidate | Native pixel | Driver | Doubled PNG |
|---|---|---|---|---|---|---|
| 60 | `1.4705882352941204 / 2.35294117647059` | V | V | H | V | V |
| 79 | `1.4705882352941204 / 1.2352941176470602` | H | H | H | V | H |
| 74 | `1.4705882352941204 / 1.529411764705881` | V | V | H | V | H |

[axis-analysis.tsv](axis-analysis.tsv) preserves raw and controlled starts/ends,
actual device deltas, native pixel deltas and doubled-PNG deltas. The measured
signature **V,H,V** supports the larger-world-delta candidate over the three
listed alternatives for this view and these paths. Small locked-component
residues are preserved in the native snapshot. Other weighted metrics,
thresholds, views and history dependence remain unproved. Existing H/V LINEs
overlap the third preview, so its current rubberband orientation was not
isolated; the saved geometry is authoritative. A provisional H preview
interpretation was corrected after export, with the original frames retained.

Exact axis equality and half-spacing ties were not certified. A decimal typed
anchor or rounded export alone cannot establish equality of native comparison
operands before conversion/rounding. Negative rounding, arbitrary spacing
and lattice origins, and ordering boundaries remain unresolved. These finite
witnesses do not authorize a general production mouse projection formula.

## Runs, cleanup and checks

The finite collector table now contains the 50 collected cases. Each stage
used its own empty directory; the selected IDs and baseline/image/QEMU facts
are in that stage's `provenance.txt`. `cancel` selected ten cases but stopped
after five at the decode failure, with the other five collected once in
`cancel-tail`.

| Directory | Actual sessions / native exports | Parsed DWGs | Process result |
|---|---|---|---|
| pilot | 9 | 9 | exit 0 |
| go | 9 | 9 | exit 0 |
| buffers | 4 | 4 | exit 0 |
| controls | 15 | 15 | exit 0 |
| constraints | 2 | 2 | exit 0 |
| cancel | 5 | 4 | exit 1, CREPEAT decode failure |
| cancel-tail | 5 | 5 | exit 0 |
| axis | 1 | 1 | exit 0 |
| Total | 50 | 49 | one preserved undecoded native export |

The default run on a fresh directory collects the finite table and reports
all decode failures at completion with nonzero status. To reproduce a stage,
copy its `selected=` list into `MENU_CONTROLS_CASES` and use a new empty
output directory with `CARGO_TARGET_DIR=target/menu-controls`. The image hash
is unchanged in successful run after-hashes; the aborted collector `cancel` stage
has an explicitly offline `postrun-system.sha256` check. Pilot drawing hashes
are in the top-level manifest; its earlier collector version had no per-case
hash files. Source evolved by adding bounded cases and evidence failure
handling during collection; actual executed inputs are the transcript. Run
provenance records baseline HEAD, not a hash of each uncommitted collector
version.

Continuation actions establish pending-state behavior; cleanup is separately
marked in the transcript. GOFIRST submits unknown `2,3` and `4,5` after GO,
then blank Return at sequence23 reopens LINE (`From point:`). GONEXT likewise
submits unknown `4,5`, then sequence23 Return reopens LINE. These unknown
submissions did not replace the remembered known LINE. Cleanup sequences
25/27/29/31 press Ctrl, press C, release C, release Ctrl; final32 confirms
Command before export. [GOFIRST continuation](go/captures/GOFIRST-023-continuation.png)
and [GONEXT continuation](go/captures/GONEXT-023-continuation.png) preserve
that history observation. RETURN, GOHIST, GOFIRST/GONEXT/GORADIUS/GOSELECT, blank
header probes and CREPEAT also have explicitly logged Ctrl-C cleanup; no
modifier remains pressed. Independent POINT continuations are saved geometry,
not cleanup that rolls back earlier entities.

Before-setting columns are inferred from typed setup and independently
exported matching BASEOFF/BASEON/default baselines. After-setting columns use
native saved headers, except CREPEAT's explicitly labelled native DXF MODE
records. Geometry columns preserve parsed Item structures; CREPEAT instead
quotes its native DXF structure and decoder failure. The artifact index links
each first probe's actual pre-press gate and post-probe state; intermediate
landings, continuations and cleanup are linked in the full transcript.

Boot/export captures preserve raw bytes and use the same graphics converter
as all frames; DOS text-mode PNGs can look garbled. DOS startup/export waits
use Session's text API, while graphics prompt claims were read from PNGs.
A presentation/crop ambiguity during viewing was resolved with explicit crops
and identical raw/PNG hashes; it was not classified as a native incomplete
capture. No timeout increase or Session/QEMU source change was made.

`verification.txt` records successful fmt/clippy, CLI refusals and offline
frame/artifact checks. No full slow oracle suite was run. Generated drawing
exports/snapshots are small; this bundle contains no disk images or font files.
