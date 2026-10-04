# Native command scripts, DELAY and RESUME

This note is the contract for task Q (command script execution). It separates
three kinds of evidence from the native policy:

- **HLP**: retained `ACAD.HLP` (`crates/acad-cmd/resources/acad.hlp`) lines
  190-198 (DELAY) and 537-541 (RESUME). They establish purpose only: DELAY is
  "used in command scripts to allow the display to be viewed before the next
  command is automatically issued", "the larger the number, the longer the
  delay"; RESUME returns "to a command script which has been interrupted due
  to an error or keyboard input". Both cite "Command scripts, Section 8.4 of
  User Guide", which is not retained. No time unit is given.
- **Strings**: `ACAD.EXE` contains `.SCR` and `Can't open script file %s`.
  AutoCAD 1.4's command list (HLP `?` page, `report.rs`) has no SCRIPT command.
- **In-tree runs**: the original `ACAD.EXE` executed by the in-tree 8086/DOS
  runner (`acad_oracle::in_tree::observe_in_tree`) with a DOS command tail and a
  script file added to the read-only System floppy's root files. The runner's
  only clock is its instruction count. Every in-tree fact below is asserted in
  `crates/acad-oracle/tests/scripts.rs`; like the rest of `acad-oracle`, those
  tests print a skip notice and pass when `System.img` is absent.

## Original behaviour (measured)

1. **Starting.** A script is named on the DOS command line, `ACAD drawing
   script`. `.SCR` is appended to a name without an extension; an explicit
   `S1.SCR` also works. A missing file prints `Can't open script file
   NOPE.SCR` and `AutoCAD gives up.` and the program exits. The script's keys
   are consumed from the **Main Menu** onwards (a first `LINE` is
   `** Invalid data entered.` there); a blank drawing name takes the command
   line's drawing.
2. **Syntax.** CR, LF and CR LF each submit one Return; outside text, each
   space and each tab also submits one Return (`LINE 0,0 5,5  END` draws one
   line and saves). At a TEXT value prompt spaces are kept (`A B`). A final
   piece without a line end is not submitted: it stays as typed input, and a
   later keyboard Return submits it. Ctrl-Z (1Ah) is not an end-of-file marker:
   it is an ordinary unknown item that stops the script, and RESUME then
   continues with the rest of the file.
3. **Errors.** An unknown command stops the script; RESUME typed at the
   Command prompt continues with the item after the failing one. An invalid
   point stops the script at the point prompt, where a typed `RESUME` is just
   another (invalid) point.
4. **RESUME** inside a running script, or after a script ended normally, is
   harmless.
5. **DELAY** takes a signed 16-bit integer. Its cost is linear: about 730
   emulated 8086 instructions per unit (1,000 units = 330-400 slices of 2,000
   instructions; 2,000 units twice that), including one DOS console poll per
   unit (the poll count at save grows by 1,000 +/- 2 per 1,000 units).
   Negative counts do not pause; 32768 and 65535 wrap negative (no pause);
   70000 wraps to 4464. `0.5`, `1e3`, `abc` and an empty answer stop the
   script rather than re-prompting: RESUME continues with the next item. The loop is CPU-bound, so its wall time depended on the host
   PC; no fixed time unit exists to copy.
6. **Keyboard interruption.** A key pressed during DELAY stops the delay and
   the script; the key remains typed input (`X` then `RESUME` is the unknown
   `XRESUME`). After erasing it, RESUME continues with the item after DELAY's
   count and does not finish the remaining delay.

## Native contract

| Topic | Native policy | Relation to original |
| --- | --- | --- |
| Starting | `SCRIPT` command (prompt `SCRIPT: file name`), `acad --script FILE` at startup, API `script {path}` / MCP `acad_script`. | SCRIPT command is a native extension; the command tail matches item 1. |
| Where items go | Items start at the editor's current prompt. There is no Main Menu, so menu selections such as `1` are unknown commands. | Divergence (no Main Menu). |
| Lookup | Absolute path; `D:rest` through the FILES drive map; else the document directory, then the process directory. No extension: `.SCR` then `.scr`. Only regular files (symlinks to them) count. Shared with external INSERT (`session/external_insert.rs`). | `.SCR` append matches; search order is native. |
| Bad file | Missing, directory, larger than `MAX_SCRIPT_BYTES` (256 KiB) or not UTF-8: refused before any item runs, with a `SCRIPT:` error. Original scripts are DOS bytes; ASCII scripts are valid UTF-8, but a file with high (CP437/Latin-1) bytes is refused rather than decoded, and a UTF-8 BOM becomes part of the first item (an unknown command). An interrupted script is kept. At startup (`--script`) the program exits with the error. | The original gives up only at startup. |
| Syntax | Exactly item 2: CR, LF, CR LF, space and tab each submit one Return; at TEXT and CHANGE TEXT value prompts the rest of the line is literal; an unterminated tail becomes typed input; Ctrl-Z is ordinary text. | CHANGE TEXT literal value is native (analogy with TEXT). |
| Submission | Each item takes the same Return route as the window and API `command` (empty Return repeats, SHP/INSERT resolution, effects). | Same keystroke model. |
| Errors | A rejected item interrupts the script. The status and `script_status.interrupt` name the cause `error`, the failing line and message; the cursor is already past the item, so RESUME continues after it. | Item 3. |
| Interruption | User input that reaches the Session while a script is running or delaying (typed characters, Return/command, backspace edits that change the input, mouse point/click, QUIT request or window close, API command/point/click/script) interrupts it before that input is handled; the input is then handled normally (a typed key stays typed). Cancel (Esc, API cancel) interrupts with cause `cancel` and also cancels the prompt. API `script_status`/`state`/`frame`/`drawing`, `report` navigation and `save` never interrupt or advance. Known GUI limit (pre-existing): keys the window ignores today (Space, Tab, arrows, Backspace on empty input) never reach the Session, so they do not interrupt, although the original stops on any key. Pointer motion (GUI move, API/MCP `motion`) is not input: it neither interrupts nor advances a script, even when SKETCH samples it into temporary strokes; a SKETCH click or control key does interrupt (native policy, unmeasured). | Item 6 for keys; mouse/API/cancel are native extensions of "keyboard input". |
| RESUME | At the Command prompt, sets an interrupted script running again at its exact next unread byte; any remaining DELAY is discarded. Inside a running script it is ignored; with no interrupted script it only reports `RESUME: no interrupted command script`. | Items 3, 4 and 6. |
| DELAY | Integer -32768..32767; negative and zero do not pause. Out-of-range integers are refused (`DELAY must be from -32768 to 32767`) instead of wrapping; non-integers are refused. Both refusals interrupt a script. **Native unit: 1 = one millisecond** on the Session's monotonic clock, so the maximum pause is 32.767 s. Outside a script DELAY validates and reports `DELAY: no command script is running` without waiting. | Range and negative rule match item 5; wrap-around and the CPU-speed unit deliberately differ. |
| Nonblocking | DELAY records a deadline only. The GUI event loop pumps the script in `about_to_wait` and sets `ControlFlow::WaitUntil(deadline)` while delaying, `Poll` while items are due and `Wait` otherwise; no event handler sleeps. | Native. |
| Bounds | One pump runs at most `SCRIPT_ITEMS_PER_PUMP` (64) items, so input and redraw interleave with long scripts. One script at a time and no queue of scripts: starting a script replaces any interrupted one; SCRIPT from inside a script is refused (error, so RESUME continues after it). | Native. |
| Exit and files | END saves and exits (script discarded); unnamed END takes the next item as its path; QUIT then `Y` exits. A failed save interrupts the script with the save error and does not exit. NEW/OPEN and `script_stop` discard the script. | Native lifecycle. |
| Clock | `ScriptClock` is injectable (`Session::set_script_clock`); the default is the host monotonic clock. Tests use a fake clock and never sleep. API OPEN keeps the session's clock. | Native. |

### API and MCP

- `script {path}` starts a script (interrupting a running one first) and runs
  due items; `script_tick {}` runs items that are due on the session clock;
  `script_stop {}` discards the script; `script_status {}` is read-only.
  MCP tools: `acad_script`, `acad_script_tick`, `acad_script_stop`,
  `acad_script_status` (read-only hint).
- Editor-input requests (`command`, `point`, `click`, `cancel`) also run due
  script work afterwards (never waiting), so `command {"input":"RESUME"}`
  continues at once. Reads (`state`, `script_status`, `drawing`, `frame`),
  `report` navigation, `save` (a document operation, not editor input),
  `new`/`open` (which discard the script) and `quit` do not.
- `state.script` and `script_status` report `state`
  (`idle`/`running`/`delaying`/`interrupted`), `path`, `next_line` (1-based),
  `next_offset` (byte), `bytes`, `items_submitted`, `delay_remaining_ms` and
  `interrupt {cause, line, message}`.
- Each call runs at most 64 items. Headless clients therefore call
  `acad_script_tick` repeatedly while `state` is `running` (any script longer
  than 64 items, with or without DELAY), and again once `delay_remaining_ms`
  has elapsed while `delaying`; MCP uses the host clock. In the attached GUI,
  the event loop advances the script itself.

## Tests

- `crates/acad-cmd/tests/scripts.rs`: DELAY parsing and range, SCRIPT/RESUME
  effects, literal-text prompts.
- `crates/acad-app/src/session/script_tests.rs` (fake clock): syntax, TEXT
  literal values, unterminated tail, DELAY deadline and no blocking, zero /
  negative / invalid counts, error line and exact resume, cancel during DELAY,
  cancel at an open prompt of a running script, cancel after an error,
  typed, Session point, click, QUIT request and input-edit interruption, API
  point/click interruption, API `script` replacing a running script, bounded
  batches, oversized/missing/binary/directory files, `.SCR` lookup, nested
  SCRIPT, inert RESUME/DELAY, END/unnamed END/QUIT, failed save, replacement
  and NEW, API/MCP state, tick (repeated for long scripts) and stop, reads /
  report / save not advancing, API OPEN keeping the clock.
- `crates/acad-app/tests/mcp.rs`: script tool list and read-only hints.
- `crates/acad-oracle/tests/scripts.rs`: every original fact above.

## Limits

- The native unit is a policy choice; the original's per-unit wall time was
  never measured on real hardware.
- Interruption by mouse, cancel and API input, the CHANGE TEXT literal prompt
  and resume after a cancel are native extensions without original runs.
- Scripts cannot drive the original's Main Menu, configuration or file
  utility screens.
- Original screen output during a script (prompt echo) is not compared: the
  editor draws prompts in CGA graphics, not DOS console text, so "X stays
  typed" and "RESUME at a point prompt is a point" are inferred from saved
  drawings (a RESUME that works after erasing X; no save), not from echo.
