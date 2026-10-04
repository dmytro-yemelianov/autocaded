# Native files, backups and menu macros

Contract for the remaining original file/backup and screen-menu macro
semantics (agentic row F, "Files and release"). Original behaviour cited
here is measured on the original ACAD.EXE by the in-tree 8086/DOS runner and
asserted in `crates/acad-oracle/tests/files_backup.rs` and
`crates/acad-oracle/tests/menu_macros.rs`. Those tests skip visibly when the
extracted `System.img` is absent; a skipped run is not evidence. Everything
else in this document is native Rust policy and is labelled so.

## Evidence: Main Menu and file tasks

`files_backup.rs` observes the original's DOS text output.

- The Main Menu offers exactly: 0 Exit AutoCAD, 1 Begin a NEW drawing,
  2 Edit an EXISTING drawing, 3 Plot a drawing, 4 Configure AutoCAD, 5 Make
  drawing interchange file, 6 Load drawing interchange file, 7 File
  Utilities. Task 7 and the editor's FILES both open the File Utility Menu.
- Task 1 with the name of an existing drawing warns
  `** Warning!  A drawing with this name already exists.` and asks
  `Do you want to replace it with the new drawing? <N>`. The existing file
  is untouched until the replacing drawing is ENDed.
- Task 2 with a missing name reports `** No drawing with this name is on
  file.` and `Press RETURN to return to main menu.`; no file is created.

Native scope: the Main Menu, its task prompts, the task-1 replace question
and END/QUIT returning to the menu are specified in
[main menu](native-main-menu.md). Plotting (task 3) and real configuration
(task 4) remain non-goals; the menu text layout is not reproduced.

## Evidence: END, QUIT and `.BAK`

- END writes `NAME.DWG` and keeps the previous `NAME.DWG` bytes,
  unchanged, as `NAME.BAK`. An existing `NAME.BAK` is replaced.
- END writes and backs up even when nothing changed (the new DWG then has
  the same bytes as the backup).
- A second edit-and-END backs up the first END's output, not the older file.
- The first END of a new drawing (no previous file) creates no `.BAK`.
- Task 1 replacing an existing drawing, then END, backs up the replaced file.
- QUIT (Y) writes neither the drawing nor a `.BAK`.
- WBLOCK to an existing drawing name replaces it only after `Y`; any other
  answer (`N`, or `*`) keeps the destination. Replacing it creates no `.BAK`.
  The question itself is described in the WBLOCK section below.

## Native backup policy

Rust policy derived from the evidence above:

- END, SAVE (native extension), API `save` and the unnamed-END output path
  all replace an existing destination through the same staged writer, which
  keeps the previous bytes as a backup. A destination that does not exist
  gets no backup. WBLOCK never makes a backup (evidence) and replaces an
  existing destination only after its replace question (WBLOCK overwrite
  contract below).
- Backup name: the destination with its extension replaced by `bak`,
  lower-case when the destination's extension is all lower-case and `BAK`
  otherwise (`HOUSE.DWG` -> `HOUSE.BAK`, `plan.dxf` -> `plan.bak`,
  `PLAN` -> `PLAN.BAK`). `x.dwg` and `x.dxf` therefore share `x.bak`; the
  latest replacement wins, as in the original's single-backup model.
- No backup is made when the destination itself has a `bak` extension (for
  example a corpus `DISC.BAK` opened and ENDed in place); the staged
  replacement is still atomic.
- A symlinked destination is followed: the link stays, the resolved file is
  replaced, and the backup is written beside the resolved file (same
  directory, so every rename stays on one filesystem).
- Order: (1) stage the new drawing in a create-new temporary file beside the
  destination (owner-only `0600` on unix while it is written when replacing
  an existing file), copy the destination's permissions and fsync; (2) copy
  only the bytes of the current destination into a second create-new
  owner-only temporary file (never file flags), copy its permissions, fsync,
  and rename it over the backup name; (3) rename the staged drawing over the
  destination, then fsync the directory (unix) so the renames are durable.
  The destination path always names a complete drawing: either the previous
  bytes or the new ones. If any step before (3) fails, the destination is
  untouched, both temporary files are removed and the save fails; the
  document stays attached to its previous path, dirty and with its previous
  baseline. If (3) fails after (2), the backup holds the previous bytes
  (the older backup generation is gone) and the destination is still the
  previous file. Once (3) has succeeded the save has happened: a directory
  fsync failure is not an error (a retry would back up the new bytes over
  the real previous `.BAK`). The document is attached, clean and rebased
  exactly as for any success, and the status carries
  `Saved <path>; directory sync not confirmed: <error>` (WBLOCK:
  `Wrote block drawing <path>; ...`). Unsupported/invalid-argument fsync
  errors (filesystems that cannot fsync a directory) are silent.
- Preconditions: a read-only destination, a non-regular destination, a
  macOS locked destination (user/system immutable or append-only flags, as
  Finder "Locked" sets) or an unwritable directory fails before anything is
  written. Linux `chattr +i` is not detected up front; it fails at (3). A
  backup name that is a directory fails the save at step (2). A symlink at
  the backup name is replaced as a link, never followed.
- Source revision: the backup is a byte copy, so an AC1.2 source keeps its
  AC1.2 bytes in the backup while END writes the detected source codec.

## Evidence: WBLOCK replace question

`files_backup.rs` reads the original's editor command area from the final
graphics frame with the original's own 8x8 display font (the ASCII table in
`ACAD.OVL`) and asserts:

- Order: `File name: D3`, then, only when `D3.DWG` exists,
  `A drawing with this name already exists.` and
  `Do you want to replace it? <N>`, then `Block name:`. The question comes
  after the file name and before the block name (and so before any
  selection). A new name goes straight from `File name: D4` to
  `Block name:`.
- Answers: any answer starting with `Y` or `y` (`Y`, `y`, `YES`, `Yes`,
  `YE`, `YEP`) continues to `Block name:`. Every other answer (Return, `N`,
  `NO`, `X`, `*`) ends WBLOCK at `Command:` and keeps the file; the default
  is No. Cancel at the question shows `*Cancel*` and keeps the file.
- The drawing being edited gets the same question and no other guard: after
  `Y` and `*`, its own `D2.DWG` holds the WBLOCK output, which QUIT then
  leaves in place (no `.BAK`).
- A command script (`ACAD D2 S1`) answers the question with its next item:
  `Y` replaces; an item written as the block name (`*`) declines, keeping
  the file.
- The original is not staged: `Y` empties the destination at once, before
  the block name, and a new name is created empty at the file name.
- An explicit extension (`D3.DWG`) is rejected with `*Invalid*`.

## Native WBLOCK overwrite contract

Native policy built on the evidence above (tests:
`crates/acad-cmd/tests/wblock_replace.rs`,
`crates/acad-app/tests/wblock_replace.rs`, the menu-macro case in
`crates/acad-app/src/session/tests.rs`, and `tools/check_acad_gui_api.py`):

- Order and answers mirror the original. After the file name the editor
  reports the destination (`Effect::CheckWblockDestination`); the host
  checks it at once and, when it exists, the prompt becomes
  `WBLOCK: A drawing with this name already exists. Replace it? <N>`
  before the block name. An answer whose first non-blank character is `Y`
  or `y` confirms; anything else, including Return, ends WBLOCK with
  `WBLOCK: kept existing <path>`. Prompt wording is native (the original's
  two lines are joined into one prompt).
- Declining or cancelling is atomic: no file is touched, and the drawing,
  undo history, dirty state, attachment and input line are unchanged.
- "Exists" means any directory entry at the name, including a dangling
  symlink. A directory or other non-regular destination fails at the file
  name with `WBLOCK failed: output path is not a regular file`.
- The open drawing: when the destination is the attached document's own
  file (same device and inode, so a symlink, `.`-path, relative name, or a
  case variant on a case-insensitive volume such as default macOS APFS all
  match), the question reads
  `WBLOCK: This is the open drawing's file. Replace it? <N>`. Like the
  original, `Y` replaces it. Natively the document then stays attached
  with the written output as its saved baseline (AC1.40 DWG), so dirty
  state reflects what the file holds: QUIT still asks and END rewrites the
  drawing. Its previous bytes are not backed up (WBLOCK makes no `.BAK`).
  A hard link to the open drawing also matches (same inode) and gets this
  question, but `Y` only replaces that link name with a new file: the
  open drawing's own file keeps its bytes and the link is broken, so
  nothing is lost.
- Writing: a confirmed replacement uses the same staged writer as SAVE
  (create-new temporary beside the destination, owner-only while replacing,
  permissions copied, fsync, rename, directory fsync), with the same
  read-only, non-regular, macOS-locked and symlink-following rules, but no
  backup. The destination keeps its previous bytes on any failure. Unlike
  the original, nothing is emptied before the write.
- Without a confirmed replacement WBLOCK only creates: the staged file is
  hard-linked into place, so a file that appears after the file name is
  refused with `WBLOCK failed: output file already exists; not replaced`
  (on filesystems without hard links a final existence check precedes the
  rename).
- API/MCP `command`, command scripts and menu macros submit through the
  same editor, so they meet the same question; a script or macro item is
  an answer, and only an explicit `Y` item confirms (a `WBLOCK name *`
  script declines at `*`, as in the original). There is no direct API save
  of a block, so no overwrite flag is needed.

## Evidence: screen-menu macro grammar

The original picks screen-menu items from the keyboard (INS, cursor-down,
Return), which lets the in-tree runner exercise private menus.
`menu_macros.rs` asserts:

- An empty source line occupies a menu slot.
- `*[label]macro` keeps its label and macro (the `*` is the page marker the
  native panel already uses).
- Every space and every `;` is one Return; consecutive separators are
  consecutive Returns (`line 2,2  3,3;` ends LINE before `3,3`).
- At the end of an item, pending text is submitted; no extra Return is
  added after a trailing space, `;` or pause.
- `\` pauses for exactly one user input, then the macro continues. Text
  right after the pause starts the next input; text right before it stays
  as the start of the user's input (`line 1,\7,7;` plus typed `1` makes
  `1,1`). Consecutive `\\` take two inputs. A pause satisfies object
  selection (`erase \;` with `L`).
- Cancel during a pause abandons the rest of the macro.
- A leading `*` also starts a new page: the row below `[A]` is empty when
  the next line is `*[B]...` (without `*`, B is picked there).

Retained menus: `ACAD.MNU` uses labels, `*` pages, spaces, `;` and the
single control bytes ^B ^O ^C (already implemented). `OFFICE.MNU` and
`SUBDIV.MNU` add `\` pauses, `*[label]` and blank lines; `SHUTTLE.MNU`
uses only spaces and `;`. No retained menu uses `+` continuation, `$S=`
page switches or other control bytes; those are later-release grammar and
are not implemented.

## Native menu policy

- The parser keeps blank lines as blank, inert slots and parses
  `*[label]macro`. Label/bracket errors keep their line numbers.
- Menu files are read with the regular-file-only, byte-capped reader shared
  with external INSERT (cap 64 KiB; retained menus are under 2 KiB).
- A picked macro is tokenised once; Returns are submitted through the same
  route as before. At a pause, the text before `\` becomes the visible input
  line and the rest waits. The next user submission (typed Return, GO, API
  `command`, or a submitted mouse point) resumes it. GUI typing appends to
  the prefix; API/MCP `command` text is likewise appended to the prefix
  while a macro is paused (`line 1,\7,7;` plus `command("1")` submits
  `1,1`) and replaces the input line otherwise.
- Native, unmeasured policy: entity picks that only collect objects at a
  selection pause (retained `erase \;`, `list \;`, `change \ \;`) do not
  resume; the user picks and then presses Return, which submits the picks
  and resumes. The original's pick-at-pause behaviour is not measured.
- Native, unmeasured policy: a command-script item is submitted through
  the same Return route, so it satisfies and resumes a paused macro (the
  item replaces any typed prefix). A macro step that starts SCRIPT finishes
  its remaining steps before the script's first item runs.
- Cancel (window Escape, API `cancel`, menu ^C), a new menu pick, API
  `new` (reset) and `open` (fresh session) drop a paused macro. Errors from
  individual Returns do not stop the macro (the original keeps feeding
  keys). When a user submission resumes a macro, API `command` reports the
  user's own submission; the macro's later Returns report through status,
  and any quit request among them is honoured.
- Macro Returns keep the existing editor submission route (no
  last-command repeat bookkeeping); GO keeps the physical-Return route.

## Corpus workflow

`crates/acad-app/tests/corpus_backups.rs` copies all 21 valid corpus
drawings (including `DISC.BAK`) and the three other Samples backups into a
scratch directory, ENDs each, checks the first backup is byte-identical to
the corpus file (no self-backup for `.BAK` destinations), reopens and
compares records (numeric fields within 4 ULPs, because the pre-existing
acad-dwg writers for both AC1.2 and AC1.40 may move a value by its last bit
on the first rewrite of an original file; a second END is byte-stable), and
checks the detected codec
and that no staging files remain. A missing corpus directory prints
`skipping corpus workflow, NOT validated` and fails when
`AUTOCAD_REQUIRE_CORPUS` is set; an incomplete corpus always fails.

## FILES policy

FILES host behaviour (drive mapping, wildcard list/delete, rename) remains
native policy. Rename refuses any existing destination entry, including a
dangling symlink, so it never replaces a user file. Original File Utility
screen layout and DOS 8.3 rules are not reproduced.

## Remaining limits

- No plotting or real configuration; the Main Menu limits are listed in
  [main menu](native-main-menu.md).
- WBLOCK accepts an explicit `.DWG` extension, which the original rejects
  with `*Invalid*`; native wording of the replace question differs from the
  original's two lines.
- No keyboard (INS) menu cursor in the native window; menus are picked with
  the pointer.
- Host backup naming, symlink and permission handling are native policy;
  DOS has neither symlinks nor permission bits.
- Disk-full or write-failure behaviour of the original during END is not
  measured.
