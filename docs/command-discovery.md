# Command discovery metadata

`crates/acad-cmd/resources/commands.json` is the single source for the 61
canonical command records. The build script generates typed native
`catalog::COMMANDS` records from it. The existing wasm `command_catalog` export
returns the same raw JSON; there is no separate browser catalog.

Schema version 1 retains `id`, `token`, `aliases`, and `source`, and adds:

| Field | Values and meaning |
| --- | --- |
| `category` | `drawing`, `editing`, `settings`, `view`, `inquiry`, `files`, `automation`, `help`, `devices`; current presentation policy |
| `availability` | `software` or `device_required` |
| `lineage` | `retained_dispatcher` or `additional` |
| `help_topic` | Exact retained `acad.hlp` label, or JSON `null` |
| `label_key` | `command.<id>.label` |
| `summary_key` | `command.<id>.summary` |

Only PLOT, QPLOT, and TABLET require devices. `software` means recognized with
an implemented software slice; it does not promise full retained option parity.
Categories are useful current groupings, without historical taxonomy claims.

Lineage is checked against the independent 57-spelling recovered dispatcher-help
inventory in `crates/acad-cmd/tests/help.rs` (`COMMANDS`), which resolves to 54
canonical IDs. SCRIPT, COLOR, ROTATE, SCALE, ENTITYAREA, UNDO, and SAVE are
`additional`: outside that inventory. This does not establish that those commands
never existed historically, or that they were introduced by a later AutoCAD
release. `source` continues to identify the v0.4.3 Rust dispatcher extraction;
it is distinct from membership in the retained inventory.

Help topics are obtained mechanically from the backslash-delimited labels in
`crates/acad-cmd/resources/acad.hlp`, without inferred labels. A canonical token
with a matching label records that label; otherwise it records `null`. Thus COLOR
is additional by inventory membership and still has retained help. Existing
alias resolution, including RES/RESOLUTION resolving to SNAP, is unchanged.

The build script rejects unknown enum values, missing discovery fields, malformed
or mismatched presentation keys, incorrect help labels (including invented
labels), blank provenance, duplicate identities, and duplicate spellings. Native
metadata and JSON are compared in catalog tests, along with the complete command
identity/alias inventory. Retained help and existing menu macro tests preserve
their current behavior.

This change supplies metadata and presentation keys, without duplicating summary
prose. A future localized discovery UI must resolve both presentation keys in the
message catalog before exposing translated discovery. The native public records
and existing wasm raw metadata export are the current consumers.
