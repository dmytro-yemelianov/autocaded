# Native API and MCP notes

Protocol and session semantics for the local socket API and the MCP adapter,
moved verbatim from the README. Start-up commands and the tool table stay in
the [README](../README.md#native-api-and-mcp).

## Socket protocol

Each socket connection sends one newline-delimited JSON request, for example
`{"method":"command","params":{"input":"LINE"}}`, and receives
`{"result":...}` or `{"error":"..."}`. The socket has mode `0600`, refuses an
existing path, and is removed on normal shutdown. Submit one prompt answer per
call and inspect `state`; request errors retain the drawing. A timed-out mutation
must not be automatically retried because it may already have run.

## MCP transport

These commands are server entry points for an MCP client, not interactive terminal
prompts. The adapter uses the
[MCP 2025-11-25 initialization and tool protocol](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
over stdio; older handshake versions are also negotiated. Reports go to stderr
and structured tool results, keeping stdout exclusively for protocol messages.

## Quit, open and dirty state

Reply to QUIT with `command {"input":"Y"}` or `YES`; other answers or `cancel`
keep the session open. Automated teardown can use `quit {"discard":true}`.
To save and exit, submit `command {"input":"END"}` (then a path if unnamed).
Explicit API `new`/`open` replace the current drawing without a confirmation
dialogue. Successful `save` attaches its path and clears `dirty`; failed saves
retain the previous attachment and baseline. Dirty state compares drawing data,
so UNDO back to the saved drawing clears it; reports and frame requests do not
make a drawing dirty.

## Report viewer

Report navigation changes only the viewer. `close` retains the complete text
for `open`; the next editor effect or `new`/`open` clears it. `cancel` cancels
the editor prompt and clears the report. Supply the dimensions of the frame
being viewed for matching page sizes. GUI empty Return closes a visible viewer;
API `command` always submits editor input, so automation should use `report` to
close or navigate. `report_view.anchor` is an opaque display-text position, not
an offset to slice the original report.

## Frames

Frames include the client drawing, menu, command area, selection and cursor;
OS title bars are excluded. PNG is returned as an MCP image. Raw frames contain
base64 RGBA8 bytes, top-down rows, opaque alpha and stride `width * 4`. Poll
`frame` for successive buffers; no video encoder or push-stream is bundled.
Frame/click dimensions default to the live window, or 800×600 when headless.
Transport frames are limited to 4096 per dimension and 4,194,304 pixels; provide
smaller explicit dimensions for a larger Retina window. The GUI composer retains
support for larger physical windows. `tools/acad_api.py --output` decodes frame
bytes directly into a PNG or raw file. Font diagnostics accompany captured frames.

## Rust API and module layout

The public Rust API exposes `Session::new/open`, `command`, `point`, `click`,
`drawing`, `prompt`, `status`, `save`, `document_path`, `document_format`,
`is_dirty`, `request_quit`, `report_text`, `report_visible`, `report_action` and
`frame`; `Frame` provides RGB words for the
window plus `rgba()` and `png()` for callers. The app is split into session
commands/resources/effects/mouse modules, document I/O, bitmap painting,
presentation, a separate report viewer with cached wrapping and painting,
window adapter, typed API, socket bridge and MCP adapter. Command
prompt handling is split by domain; HATCH clipping, area metrics and editing
geometry are separate modules. Existing native-export parity tests still run
against the same recovered algorithms.

## Attached-window regression

Run the attached-window regression on a Unix desktop with a graphical session:

```sh
cargo build -p acad-app --bins
python3 tools/check_acad_gui_api.py
```

It creates scratch drawings, exercises the real GUI through both socket/API and
MCP, captures PNG/RGBA at three sizes, checks DWG/DXF reopen and UNDO, declines
QUIT, then saves/exits with END and checks socket cleanup. Logs, frames and the
reopened state remain in the printed temporary directory. It also checks report
pages, resize, mouse paging, the CLI report method and drawing/undo neutrality.
The drawing includes numeric-D, 2P and 3P CIRCLE forms, a triangular SOLID,
LINE/ARC continuation and a center/angle ARC.
It does not send
desktop mouse/keyboard events or write source fixtures.

## Headless corpus regression

Run the retained corpus through headless Session/MCP with the same binaries:

```sh
python3 tools/check_acad_corpus_api.py
```

This requires all 21 drawings, three additional Samples backups and SUBDIV.DXF,
with manifest hashes intact; missing files fail the check. It captures frames,
checks Save As AC1.40 and END's attached revision, and reopens scratch DWG/DXF
files in fresh processes. DWG canvas pixels and canonical DXF text must remain
stable. Historical DXF rounds to six decimal places and omits AXIS, so its pixel
differences are recorded separately. Results, PNGs and outputs remain in the
printed temporary directory. This workflow passed all 25 inputs at the current
native checkpoint; broader file/command compatibility work remains open.
