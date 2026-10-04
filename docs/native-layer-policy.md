# Native layer visibility and signed persistence contract

Retained `crates/acad-cmd/resources/acad.hlp` LAYER topic (lines 369–381)
supports numeric current layers, ON/OFF comma lists, COLOR x and ?. The COLOR
help page (lines 158–172) establishes colors 1–7. These dialogue forms now
execute in Rust, either after a LAYER prompt or as `LAYER OFF 1,2` / `LAYER
COLOR 3`. The separate existing COLOR dialogue remains available.

## Native behavior contract

- Layer indices remain 0–127 and colors 0–254, preserving existing accepted
  ranges. Numeric LAYER defines an absent slot with the existing color 15.
- ON/OFF requires defined layers; malformed, empty list elements, out-of-range
  and undefined layers fail before any drawing or undo mutation. Duplicate
  indices are harmless. A bad list remains at its list prompt for retry/cancel.
- Turning the current layer OFF is allowed, and drawing on that layer stores
  entities while hiding their geometry. Choosing a hidden current layer leaves
  it hidden. These acceptance choices are native Rust policy, not established
  original behavior.
- Visibility is separate from color in `Header.off_layers`, included in model
  equality, document dirty state and UNDO snapshots. Actual layer/color/current
  changes each create one undo snapshot; repeated no-ops and reports create none.
- `LAYER ?` reports every defined layer's color, ON/OFF state and current marker
  through the shared report effect. API state exposes current_layer, layers and
  off_layers; API drawing/export preserves supported signed OFF state and
  reports an encoding error for unrepresentable native edge cases.
- `Header.layer_is_visible`, `entity_is_visible` and `item_is_visible` provide
  reusable gates. Ordinary geometry/INSERT without a layer wrapper uses layer
  1, matching existing model and codec defaults. Bare REPEAT markers have no owner layer: children filter independently,
  so turning OFF layer 0 still shows explicit visible-layer children. Empty
  bare repeats are invisible to the helper. An outer OnLayer wrapper gates an
  entire INSERT/REPEAT; child explicit layers also gate their geometry. Bare
  block/repeat geometry children use layer 1. The INSERT inheritance policy is
  native Rust behavior. For REPEAT the marker rule is original behavior (E2,
  oracle `repeat_layer.rs`): with the marker layer OFF the original still draws
  the pattern and windows still select its members; with the member layer OFF
  it draws and selects nothing. The original has no group owner layer: CHANGE
  layer rewrites each member and keeps the marker layers. An explicit REPEAT
  owner wrapper (library-built only) is persisted only when every member
  already carries its layer, so its gate equals the members' own; CHANGE layer
  drops it ([group persistence E2](native-group-persistence.md)). LOAD records preserve their ordered font/shape
  effect even on hidden layers or inside hidden groups.
- Renderer flattening applies these gates across INSERT and REPEAT. Canonical
  IDs count live stored owners independently of visibility. Explicit numeric,
  ALL and LAST selectors retain hidden IDs and may edit those records; mouse
  picks/windows and highlight geometry exclude hidden owners and members.
- Windows aggregate only visible geometry into compact two-point bounds, then
  compose repeat lattice extremes and conservative transformed INSERT boxes.
  No generated repeat cell is allocated. Bounds visits are capped at 100,000
  stored records and 256 stored nesting levels, with 16 nested INSERT references;
  a failed bound produces no partial object selection. Text/shape windows use
  their existing origin-point policy, circles/arcs conservative radius boxes.
- Picking keeps the existing 100,000 member/32-depth repeat budget. INSERT
  origin picking is retained as native policy only when its block has visible
  geometric descendants; missing, empty or all-hidden block contents produce no
  origin hit. Full transformed INSERT-geometry picking remains a separate gap.
- Selected highlights walk original document order while retaining LOAD effects
  inside preceding hidden/unselected INSERT/REPEAT owners. The resource walker
  is shared with ordinary rendering, rather than cloning a filtered drawing.
  Selected owners preflight a conservative 100,000 generated-record visit cap,
  256 stored levels and 16 INSERT references before repeat-cell expansion. This
  is a record-work cap, not a primitive/stroke cap. Oversized owners emit no
  partial highlight. LOAD-only traversal visits at most 100,000 stored records
  per owner and 256 stored levels without expanding cells; a failed context
  suppresses the selected highlight instead of using stale font state.
- ZOOM Extents/All fit visible geometry only (V1,
  [view policy](native-view-policy.md)), so OFF layers do not enlarge the view.

## Historical signed persistence L3

The retained static audit in
[2026-10-04-off-mapping-review.md](superpowers/reviews/2026-10-04-off-mapping-review.md)
establishes signed NEG of a defined positive color as OFF. DWG AC1.2/AC1.40
store the 128 little-endian signed table words at 0xC8; historical comma DXF
LAYERC reads/writes signed decimal values. OFF color 7 is 0xFFF9 (`f9 ff`), or
`-7` in LAYERC, rather than a guessed flag bit. The audit independently checked
retained program bytes and field descriptors; none of the 24 corpus DWG/BAK files
contains a negative table value. These are static program facts, not measured
original OFF export/reopen parity. The AC1.2 evidence establishes the retained
AutoCAD 1.4 reader's handling of that table, rather than a separately measured
AutoCAD 1.2 producer.

Checked persistence now supports defined layers 1–127 with colors 1–127 in OFF
state. Decoding negative words retains their positive magnitude in `layers` and
records their slot in `off_layers`; encoding negates that magnitude. Positive
colors 0–254 retain their existing native semantics, and positive 255 still
means an unused slot. Invalid negative magnitudes, including -255 and -32768,
fail explicitly. No checked negation can overflow.

OFF layer 0, color 0, colors above 127, undefined slots and out-of-range slots
remain checked encoding errors. Zero has no negative-zero representation; native
high colors and layer 0 lack the bounded historical command/renderer evidence.
Both DWG writers and checked DXF `try_write` validate before staging replacement
files. The legacy DXF `write -> Vec<u8>` adapter delegates to `try_write` and
panics on an unrepresentable state. No writer clears OFF or silently drops it.
DXF also rejects invalid sparse table indices or the reserved 255 color instead
of silently dropping model entries.

SAVE, END, WBLOCK and API save/export preserve supported OFF state on reopen.
Successful saves update the document baseline normally. Encoding failures retain
existing destination bytes, attachment, dirty state, in-memory visibility and
undo history; API save/export also retains pending prompt/input. Turning layers
ON or UNDO restores saveability for color-zero OFF. Selecting an OFF current
layer still leaves it OFF under the established Rust command policy; codec
roundtrips do not adopt the original command's recovered automatic ON behavior.
Broader original dialogue/policy parity remains a separate gap.

## Validation anchors

`acad-cmd/tests/layers.rs` covers atomic retry, duplicates, no-ops, report neutrality,
current-layer OFF, compatibility COLOR dialogue and undo, plus both DWG revisions.
`acad-render` visibility and font placement tests cover mixed nested INSERT/REPEAT
geometry and LOAD ordering. Both codec `tests/layer_visibility.rs` suites use synthetic static-contract
fixtures for exact signed words/text, malformed negatives, positive color range,
unused slots, checked/legacy refusal and exact positive header-table reencoding
for all 24 retained DWG/BAK files. App layer/API and lifecycle tests cover
shared frames/state/reports, supported SAVE/END/WBLOCK/API export and reopen,
color-zero failures, intact files, dirty/attachment/prompt state and ON/UNDO
recovery. These are native Rust
contracts; no guest/oracle/translated-runtime process is run.

Validation on the isolated L1 copy: the final six native packages passed 424
tests with no failures/ignored tests. Six-package all-target Clippy with warnings
denied passed; final model/render/command Clippy, changed-file rustfmt checks and
diff checks passed. Independent review caught and corrected a bare-record
default-layer mismatch: bare geometry/INSERT uses the existing codec default
layer 1, with new rendering/reopen agreement coverage. Corpus symlinks were
present; retained native comparison tests ran without tolerance changes. Full
application picking remains an integration check owned by the selection and
coordinating milestone.

Combined visibility integration validation: all affected acad-cmd/acad-render/
acad-app suites passed331 tests, zero failed/ignored; focused owner/window/pick/
highlight/API suites passed149 tests. Scoped all-target Clippy with warnings
denied and changed-file rustfmt/diff checks passed. The ordinary resource-aware
renderer walker is extracted into a focused module; selection budget preflight
is separate. No codec or historical persistence mapping changes are part of
this integration patch. Root owns combined codec/selection/visibility gates.
