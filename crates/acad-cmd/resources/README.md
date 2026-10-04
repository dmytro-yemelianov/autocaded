# Retained command help data

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
