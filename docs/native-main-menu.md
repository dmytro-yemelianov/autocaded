# Native Main Menu

Contract for the startup task menu (agentic row M1). Original behaviour
cited here is measured on the original ACAD.EXE by the in-tree 8086/DOS
runner and asserted in `crates/acad-oracle/tests/main_menu.rs` (this
document) and `crates/acad-oracle/tests/files_backup.rs`
([files/menu](native-files-menu.md)). Both skip visibly without the
extracted `System.img` and fail when `AUTOCAD_REQUIRE_CORPUS` is set; a
skipped run is not evidence. Everything under "Native policy" is Rust
policy. Plotting is out of scope by user decision.

## Evidence: the original Main Menu

- Startup shows `Main Menu` with exactly, in order: `0.  Exit AutoCAD`,
  `1.  Begin a NEW drawing`, `2.  Edit an EXISTING drawing`,
  `3.  Plot a drawing`, `4.  Configure AutoCAD`, `5.  Make drawing
  interchange file`, `6.  Load drawing interchange file`, `7.  File
  Utilities`, then `Enter selection: `.
- Tasks 1, 2, 3, 5 and 6 ask `Enter NAME of drawing: `. After a name has
  been given the prompt is `Enter NAME of drawing (default A:D2): ` and the
  menu shows `Current drawing:  A:D2` (also after a failed task 2).
- A blank or non-numeric selection prints `** Invalid data entered.` and
  `Press RETURN to return to main menu.`. Other numbers (9, 12) redisplay
  the menu without a message.
- A blank name with no default prints `Improper name for drawing.` and the
  same RETURN line; nothing is written.
- Task 0 prints `End AutoCAD.` and the program exits.
- END and QUIT (Y) in the drawing editor return to the Main Menu, showing
  `Current drawing:  A:D2`; the program keeps running. The original's own
  HELP END / HELP QUIT text says both return you to the Main Menu.
- Task 1 over an existing name warns and asks `Do you want to replace it
  with the new drawing? <N>`; `N` returns straight to the Main Menu with the
  file untouched; `Y` edits a blank drawing and END backs up the old file
  (`files_backup.rs`).
- Task 2 with a missing name: `** No drawing with this name is on file.`
  (`files_backup.rs`).
- Task 4 shows `Configure AutoCAD.`, `Current AutoCAD configuration` with
  `Video display:`, `Digitizer:` and `Plotter:` lines, `Press RETURN to
  continue:`, then `Configuration menu` with `0.  Exit to Main Menu`,
  `1.  Show current configuration`, `2.  Allow I/O port configuration`,
  `3.  Configure video display`, `4.  Configure digitizer`, `5.  Configure
  plotter`, `6.  Configure system console`, `7.  Configure operating
  parameters`. Its 0 asks `Keep configuration changes? <Y>`.
- Task 7 shows the `File Utility Menu`, text-identical (title and all eight
  items, `0.  Exit File Utility Menu` to `7.  Rename files`) to the editor's
  FILES menu; its 0 returns to the Main Menu.
- Task 3 asks for the drawing name, then shows the plot settings and
  `Do you want to change anything? <N>`.
- Task 5 (Make DXF) on `D2` prints `Make drawing interchange file`,
  `Drawing interchange file complete.` and the RETURN line, and writes
  `D2.DXF` beside the drawing. The DWG is only read. The text is exactly
  the native `acad_dxf::write` output for that drawing (CR LF, `^Z`
  terminator) followed by NUL padding to a 128-byte record. An existing
  `D2.DXF` is replaced without a question and no `.BAK` is made. A missing
  drawing reports `** No drawing with this name is on file.` and writes
  nothing.
- Task 6 (Load DXF) on `D3` prints `Reading drawing interchange file`,
  `End of drawing interchange file.` and the RETURN line, writes `D3.DWG`
  and stays at the Main Menu. Edited and ENDed by the original, that
  drawing holds the DXF's entities.
- Task 6 into an existing drawing does not ask and makes no `.BAK`: the
  DXF's entities are appended after the drawing's own. A DXF carrying a
  LIMITS record (the original's full task-5 output) replaces the drawing's
  LIMITS. An entities-only DXF (no header records) keeps the drawing's
  LIMITS, text size, trace width, snap, layer table and current layer.
- Task 6 of an entities-only DXF into a new name gives, after the original
  edits and ENDs it, LIMITS (0,0)-(10,10) and snap on at 0.25, which are not
  the NEW-drawing defaults.
- Task 6 without the DXF: `Could not open file A:D2.DXF`; an existing
  drawing of that name is unchanged.
- Task 6 format errors print `Format error in A:D2.DXF, line N:` and the
  offending line. An existing drawing is unchanged; with no drawing of that
  name the original leaves an empty `D3.DWG` behind.

## Native policy: when the Main Menu appears

- `acad` launched with neither a drawing path nor `--script` starts at the
  Main Menu (with or without `--api-socket`). Font directories are
  positional after a drawing, so they imply a drawing.
- The macOS `.app` bundle starts with a writable copy of `WELCOME.DWG` when
  launched without a drawing or script. Demo copies live in
  `~/Library/Application Support/AutoCADED/Drawings` and existing files are
  preserved. Bundled font and menu resources do not require the extracted corpus.
- `acad DRAWING [fonts...]`, `acad-mcp --drawing PATH` and headless
  `acad-mcp` (empty unnamed drawing) start in the drawing editor exactly as
  before; their END and QUIT exit the process. `acad --script FILE` without a
  drawing keeps the historical sample drawing (`corpus/Samples/SUBDIV.DXF`
  relative to the working directory).
- API/MCP `main_menu` (tool `acad_main_menu`) shows the Main Menu in any
  session, headless or attached, when the drawing has no unsaved changes;
  otherwise it fails and nothing is discarded. It also makes END/QUIT
  return to the Main Menu from then on. API `open` keeps that policy; API
  `new` leaves the menu for a fresh unnamed drawing.
- Once a session returns to the Main Menu, typed END/QUIT, a script's END
  and a menu macro's END return to it. The rest of a script or macro is
  dropped (native deviation: the original keeps feeding script keys to the
  Main Menu). Window close and API `quit`
  still exit the process: at the Main Menu immediately (nothing is
  unsaved), in the editor after their Y/YES confirmation. API
  `quit {discard:true}` exits immediately as before.

## Native policy: the shared route

- The Main Menu is Session state. The window's typed Return, API/MCP
  `command` and the shared `prompt`/`input`/`status` all use it; the GUI
  paints its text screen above the command line, and `state.main_menu`
  reports `screen` (`selection`, `drawing_name`, `replace`, `acknowledge`,
  `configure_show`, `configure_menu`, `configure_acknowledge`,
  `file_utilities`), `task`, `tasks`, `current_drawing`, `messages` and
  `text` (`null` in the editor); `state.returns_to_main_menu` reports the
  END/QUIT policy. Failed tasks also return the message as the `command`
  error, as editor errors do.
- Escape/API `cancel` also abandons a pending window-close/API-quit
  confirmation, so a later typed END/QUIT still returns to the Main Menu.
  API `main_menu` while File Utilities is open closes the FILES dialogue.
- Escape/API `cancel` returns to `Enter selection` (inside the
  configuration menu's message, to the configuration menu). Points, clicks
  (other than paging the FILES report), SAVE and SCRIPT are refused at the
  Main Menu with a message and change nothing.
- Names: `X:NAME` uses the FILES drive mapping (`AUTOCAD_DRIVE_X`, `A:` the
  working directory); other names are relative to the working directory or
  absolute. Without an extension the task's extension is added (`DWG`, or
  `DXF` for the task 6 source), preferring an existing upper- or lower-case
  spelling, else upper case. An explicit extension is kept, so task 1/2 also
  accept `.dxf` drawings (codec by suffix/magic, as API open/save). Names
  ending in a separator, with `*`/`?`, or an empty `X:` are improper and
  (unmeasured in the original) never become the current drawing. Names are
  otherwise recorded before the task runs, so a failed task 2 still sets
  the current drawing (evidence). The default name after END/QUIT is the
  document's full path.
- Task 1 attaches a blank drawing to the path and writes nothing until END,
  which replaces an existing file through the staged backup writer
  ([files/menu](native-files-menu.md)). A missing directory is reported.
- Task 2 opens through the same reader as `acad DRAWING`; decode errors are
  reported and change nothing.
- Task 5 writes `NAME.DXF` (case following the drawing's extension) with
  `acad_dxf::try_write`, through the staged writer without a backup (as the
  original: also avoids `NAME.BAK` holding DXF bytes in place of the
  drawing's backup). No DOS record padding is added. Encoding errors write
  nothing.
- Task 6 parses the DXF with `acad_dxf::parse` and writes `NAME.DWG`
  (AC1.40). An existing drawing is decoded and the DXF's items are appended.
  Header merge is per record: only the header records the file contains
  (`EXTENTS`, `LIMITS`, `BASE`, `DWGVIEW`, `MODERES`, `MODEGRID`,
  `MODEORTHO`, `MODEFILL`, `TXTSIZE`, `TRACEWID`, `LAYER`, `LAYERC` as a
  whole table with its off layers, `DIMARROW`) replace the drawing's; the
  FILLET radius, units, axis and DWG header passthrough bytes have no 1.4
  DXF record and are always kept. Partial-header files other than the two
  measured cases are native policy.
- Native safety extra (not original behaviour): when task 6 rewrites an
  existing drawing, its previous bytes are kept as `NAME.BAK` through the
  same staged backup writer as END (replacing an older `NAME.BAK`). A new
  drawing is written without a backup.
- Native difference: a new drawing from task 6 starts from the native
  NEW-drawing header (LIMITS (0,0)-(12,9), snap off at 1) before the DXF's
  header records apply, not the original's measured (0,0)-(10,10) / snap
  0.25 defaults.
- Native refusals, with nothing written: a DXF format error (the original
  would leave an empty new drawing), an existing drawing that cannot be
  decoded, and a block name defined in both.
- Task 3 lists Plot but reports `plotting is not available in the native
  application` and the RETURN line.
- Task 4 shows the native configuration (window display, host pointer and
  keyboard, no plotter), `Press RETURN to continue`, then the original
  configuration menu. 1 shows the configuration again, 2-7 report that the
  item is not configurable natively, 0 returns without the `Keep
  configuration changes?` question (nothing can change).
- Task 7 runs the editor's FILES dialogue on a blank drawing; when FILES
  leaves (selection 0 or Escape) the Main Menu returns.

## Remaining limits

- No plotting (task 3) or real configuration (task 4); the original's
  screen layout, banner and DOS 8.3 name rules are not reproduced.
- R5 (done): drawings made by the original's task 6 and never re-saved by
  the original hold zero-filled erased placeholder records, one per DXF
  header record, including an unpaired erased ENDREP. They now open through
  task 2, `acad DRAWING`, API `open` and MCP `--drawing`, and native task 6
  can append to them. The structural placeholders (erased ENDREP, BLOCK and
  ENDBLK) are dropped on read, as the original's END drops them. The
  ordinary ones remain non-live erased records and are saved byte for byte,
  under the existing erased-record policy.
  docs/native-group-persistence.md "Task 6 placeholder records" has the
  rule, the evidence and the one remaining limit: a lone erased REPEAT
  start, written for a DIMARROW record that is not followed by MODERES, is
  still refused.
- Disk-full and write-failure behaviour of the original's tasks 5/6 is not
  measured; native failures leave the destination untouched (staged write).
