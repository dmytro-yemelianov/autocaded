# Retained command help data

`commands.json` is the canonical, versioned command/alias catalog, extracted from
the v0.4.3 Rust dispatcher. `build.rs` validates it and generates the identities
and metadata used by command dispatch. See [data-driven evolution](../../../docs/data-driven-evolution.md)
for its schema and localization plan. HELP's historical text below is retained.

`acad.hlp` is the textual portion of `corpus/System/ACAD.HLP` from the retained
AutoCAD 1.4 System corpus. Original file: 26,240 bytes; SHA-256
`7495be58980e27bf1557ab2fed1ffcaf3a4a64e5767afcf9542c1b543d9ad1a5`.
The first DOS EOF is at byte 26,137. Bytes from EOF onward are omitted and
CRLF is normalized to LF; all text before EOF is otherwise unchanged.
This shipped resource makes HELP independent of the ignored corpus directory.

`report/help.rs` indexes backslash-delimited topic labels. Consecutive labels
(`DIM`/`DIMENSION`, `COLOR`/`COLORS`, `HELP`/`?`) share the following body.
Returned pages trim leading/trailing blank lines and end with one newline,
preserving internal indentation and wording. The existing command-list output
is retained. RES/RESOLUTION use the SNAP page because they alias SNAP in the
recovered dispatcher; this topic mapping is a Rust policy, not a file label.

The existing LINE page still compares exactly against its earlier retained
text constant. Tests cover all 57 dispatcher names, aliases, final-page EOF,
case/whitespace, invalid-topic recovery and drawing/undo neutrality.
Only LINE had an original-program help-screen comparison; the other pages
have retained-file coverage without a screen/paging parity claim. Their text
describes the original feature set, including options not yet implemented;
see [command audit](../../../docs/native-command-matrix.md).

`commands.json` and `hatch-patterns.json` are versioned, language-independent
catalogs. Their `source` fields record the implementation snapshot each entry
was extracted from; build scripts validate them and generate Rust lookup data.

`messages.json` is the single presentation catalog. Its schema version 1 uses
stable dotted keys, text-only named arguments, English templates and optional
Ukrainian templates. The shared build parser rejects duplicate/unknown JSON
fields and validates identity, argument, template and resource budgets before
generating typed Rust messages and static lookups. Templates support `{name}`,
`{{` and `}}`; substitution inserts arguments literally. Missing Ukrainian
uses the complete English entry. There is no runtime JSON formatter.
See [message contract](../../../docs/message-catalog-contract.md) for the exact
22-entry first slice and deferred consumer/translation work.

Command discovery fields (`category`, `availability`, `lineage`, `help_topic`,
`label_key`, and `summary_key`) are documented in
[command discovery](../../../docs/command-discovery.md). Category is current
presentation policy, software availability is implemented-slice availability,
and lineage refers specifically to the independent retained dispatcher-help
inventory. Presentation keys carry no duplicated summary prose.
