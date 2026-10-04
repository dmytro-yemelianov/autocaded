# Native selection and selected LIST contract

This milestone defines a Rust editor contract. Retained ACAD.HLP LIST describes
selected-object database information; REPEAT/ENDREP describe grouped rectangular
patterns. Neither retained page establishes one-based IDs or group picking.
LIST now asks for objects before producing its bounded report. The retained
HATCH window fixture sequences complete at the second corner; that completion
policy remains explicit in the shared selector. General window and multiple-pick
completion are Rust contracts, informed by retained selection/Return observations
rather than claims of full original dialogue parity.

A selectable object is a live top-level Entity other than LOAD, or one top-level
REPEAT group. IDs follow drawing order and are recomputed after edits; block
records, erased records and LOAD metadata receive no IDs. Nested REPEAT members
and translated lattice instances share their owning top-level ID. LIST, numeric
selection, ALL/LAST, HATCH boundary IDs, picks, highlighting and API
`selectable_objects` use the same traversal. The existing API `entities` value
continues to count live stored entity records (including LOAD and repeat members).

The shared selector serves editing, BLOCK/WBLOCK, ENTITYAREA, HATCH and LIST.
An explicit numeric/comma-separated ID list, ALL or LAST executes immediately
and replaces any collected set. Mouse picks accumulate sorted, deduplicated
owners. W/WINDOW asks for two inclusive corners, adds every wholly contained
visible owner, and returns to collection until physical Return. More windows or
picks can extend that set. HATCH alone completes its window at the second corner,
preserving the complete retained HNETDEF/HLINCIRC command sequences. Its mouse
picks still collect until Return. Window corners bypass SNAP/ORTHO; the opposite
corner accepts relative coordinates. Conservative INSERT/group bounds exclude
partially contained owners. Misses, invalid input and empty windows retain the
pending selector; failed cardinality/geometry validation preserves a collected
set for typed replacement or cancel. Cancel and selection collection do not
change geometry, view, document dirty state or undo history.

LIST accepts one or multiple owners through this selector and retains their
global IDs in the report. Return without picks cancels an empty LIST. API/MCP
`point` at a selection prompt collects an exact world-space hit (zero tolerance);
pixel clicks use the existing viewport picking tolerance. Both transports share
the same set and visible highlights. Reports remain complete through API/MCP and
paged in the GUI independently of editor prompts.

MOVE/COPY/SCALE, ERASE/OOPS, BLOCK/WBLOCK and ARRAY handle whole groups. ROTATE
rejects selected REPEAT lattices before mutation/undo because axis-aligned stored
spacing cannot represent general rotation. CHANGE point, FILLET, BREAK,
ENTITYAREA and HATCH retain their supported geometry restrictions, with explicit
rejection for groups. CHANGE layer applies the target layer to every stored
descendant record for both group representations, since a stored REPEAT marker layer is metadata, not a Rust owner gate.
Circular ARRAY keeps the existing translation-only policy, finds the first
geometric member after metadata, and refuses groups without a geometric anchor.
Selected WBLOCK retains ordered LOAD metadata and the reachable block closure,
including dependencies through nested INSERTs.

Picking checks translated repeat instances without expanding the drawing, up to
100,000 visited member instances and 32 repeat levels per object. A group that
exceeds these limits has no mouse hit; it remains selectable by ID. INSERT still
uses its origin for picking. Window containment uses all lattice corner extrema
and excludes empty/unsupported groups. Bounds refresh and window containment
aggregate cumulative minimum/maximum lattice offsets while visiting each stored descendant once. An explicit iterator
stack uses O(stored nesting depth) auxiliary memory and returns only two extrema;
per-record scratch contains at most four points. Neither generated cell counts
nor nesting multiply the returned point vector.

LIST retains its compact status summary and reports layer and applicable
coordinates, radius, angles, text/shape/block identity, scales and repeat counts/
spacing. It never expands block/repeat contents. Reports detail at most 1,000
objects and approximately 256 KB, with explicit omitted-object counts. String
fields escape control characters and truncate after 160 escaped characters.
The field layout and bounds are Rust policies, without original output parity.

Canonical numbering remains independent of visibility. Explicit numeric IDs,
ALL and LAST may select hidden records for inspection or editing. Mouse picks,
windows and highlights filter owner and child geometry using the shared layer
policy without allocating new IDs. INSERT-origin picks require visible block
geometry; windows use conservative transformed visible bounds. Selected
highlights preserve preceding LOAD effects from hidden/unselected owners and
reject work beyond the documented generated-record/context budgets.

## Group persistence

B3a preserves independent REPEAT and ENDREP records, their separate marker
layers, and every child's actual layer header. Stored live nested patterns are
supported in both DWG revisions and historical DXF, including patterns inside
blocks and ordered LOAD metadata. Marker metadata never introduces a runtime
owner visibility gate. Command REPEAT captures its opening current layer;
ENDREP captures its closing current layer. INSERT references retain independent
block definitions and do not count as lexical nesting.

The native codec preflight limits stored logical records to 65,535 and combined
block/repeat nesting to 64. It visits stored records with an iterator stack before
serialization and never expands repeat cells or INSERT references. Invalid
empty/zero-dimension/nonfinite-spacing groups, stacked layer wrappers and
explicit `OnLayer(REPEAT)` owner gates are checked errors. The latter has no
established lossless file discriminator from bare marker metadata. DWG decoding
also refuses negative BLOCK markers, erased subgroups inside live blocks or
groups, mixed-sign erased owners and cross-block grouping (erased ordinary
members inside live groups and blocks are retained since B4,
[group persistence](native-group-persistence.md)). These cannot silently
disappear or reparent across block boundaries.

DWG persists whole native erased Repeat owners by negating every lexical marker
and member with the established signed physical-record rule. Only a balanced,
uniformly negative top-level subtree reconstructs `Erased(Entity::Repeat)`;
nested Repeat metadata remains beneath that single owner. Referenced block
definitions remain independently live. Both revisions retain layers, fields,
counts and no LOAD effects from erased owners. ERASE/BLOCK → SAVE/END/reopen is
usable under this explicit native policy, distinct from the original static
source-member selection path. B4 added an ordinary erased-member tag; whole-owner
erasure after prior member erasure remains a checked compatibility gap.

Historical DXF is a live-only export: it omits erased ordinary and group owners,
including their erased content and marker metadata, as complete subtrees. It is
not erased-history persistence. In-session OOPS/UNDO still restores the original
model; opening either format starts a fresh editor restoration stack.

Named and selected WBLOCK exports preserve ordered preceding library effects
from hidden/unselected stored Repeat and INSERT owners without emitting preceding
geometry. Repeat bodies are projected once; INSERT resolution has a 16-frame
cycle/depth guard, with 256 stored depth and 100,000 stored visits shared across
the context projection. Unknown/cyclic/oversized context refuses export. Selected owners and retained
definitions are preflighted before recursive clones/dependency walks. Named
exports retain the source base point and reachable definition closure. These
budgets apply to stored traversal, never generated cells or primitive count.

SAVE/END failure retains destination bytes, document attachment, dirty state and
undo; API save/export also retains pending input. Invalid ordinary nonfinite
fields and block bases are checked consistently on read/write. Existing strict
corpus comparisons and source aliases remain unchanged.

The [group persistence evidence](native-group-persistence.md) records exact
retained static descriptors, corpus byte walks, native policies and remaining
original compatibility gaps. Existing original comparisons retain their strict
byte/numeric assertions.
