# HATCH user-defined patterns (U) and external pattern files

Status: H2. The `U` prompt flow and geometry are compared with the original
program through the in-tree 8086/DOS runner; the external pattern-file route
is native Rust policy built on oracle-proven facts about `ACAD.PAT`. Island
styles (`N`/`O`/`I`) are unchanged from `docs/native-hatch-styles.md`.

## Evidence

### Help file (acad.hlp lines 308-329)

- `HATCH  Pattern (name,style / U / ?): <enter name and style>`.
- `"?" will list the standard patterns on file, and "U" prompts you to
  define a pattern on the fly:` `Angle for crosshatch lines: <angle>`,
  `Spacing between lines: <distance>`, `Double hatch area (Y/N) <N>:`.
- A standard pattern is followed by scale and angle prompts.

### Original program (in-tree runner, `crates/acad-oracle/tests/hatch_user.rs`)

Each fact below is asserted by that test. The test runs the original
`ACAD.EXE` from the retained `System.img`; when the image is absent every
test prints `skipping in-tree oracle: extracted System.img absent` and
passes without asserting.

1. `U` asks for no scale. Its angle, spacing and double answers produce the
   same block, layer-127 LINEs and INSERT as the native editor, within 1e-6
   in record order: angles 45, 0, 30, -30 and 60; spacings 1, 1.5, 0.75 and
   0.5; single and double; a square and a circle boundary.
2. The pattern is one family through the world origin at the user angle with
   the given perpendicular spacing; `Y` adds a second family at angle + 90
   emitted after the first (the same order as `NET`).
3. `u` is accepted like `U`; `Y`, `y` and `YES` mean double; Return, `N` and
   `NO` mean single.
4. `U,O` is accepted (style suffix on `U`); on a single loop it equals `U`.
5. The angle and spacing prompts accept two points: `1,1` then `2,2` is 45
   degrees and `2,3` then `2,4` is spacing 1.
6. Divergent original behaviour pinned by the test, where native policy
   deliberately differs (see below):
   - Return at the angle or spacing prompt ends HATCH with the drawing
     unchanged; there is no default.
   - Spacing `0` is accepted at the prompt; HATCH then selects the boundary
     and finishes without creating any block or INSERT. Spacing `-1` hatches
     the rows of spacing 1, swept from the centre in the opposite direction.
   - Any other double answer (for example `X`) is taken as `N`.
7. The catalogue and geometry of named patterns come from `ACAD.PAT` at run
   time, not from tables in `ACAD.EXE`: with a copy of the System image in
   which `*line` spacing `.125` is patched to `.250`, `LINE` hatches at 0.25
   (equal to native `LINE` at scale 2); renaming `*net,` to `*zzz,` makes
   `ZZZ` hatch exactly like native `NET` while `NET` and an unknown name
   (`FOO`) end HATCH with no block. The file's `ANSI31`, `ANSI37` and `BRICK`
   definitions, which are not in the native built-in catalogue, are accepted
   by the original and equal the native external-file route given the
   retained `ACAD.PAT`.
8. Sweep start (shared by `U` and named patterns): the first row is the one
   whose index is `(centre - phase) / spacing` truncated toward zero, then
   the sweep runs up and then down from it. Checked for `U` at 0 and 180
   degrees (spacings 1.2 and 1.6), `LINE` at scale 9.6 and angles 0 and 180,
   and `PLAST`, `PLASTI`, `TRANS` and `INSUL` on a unit square. The native sweep used to
   round this index, which started some rows one step off; it now truncates.
9. Row orientation (continuous rows): a row's spans and each span's
   endpoints run in ascending x when the row is closer to horizontal than
   vertical and in ascending y otherwise (diagonals by y), whatever the
   pattern direction. Checked for `U` double hatches at 100, 135, 150, 180,
   200, 225, 260, 270, 300, 315 and 330 degrees around a square hole, and for
   `LINE` at 180 degrees. The native sweep used to emit rows in the pattern
   direction (for example right to left at 180 degrees); continuous rows now
   follow the original. Dashed rows are not covered by this evidence and keep
   the pattern direction.

Items 8 and 9 change the shared sweep for all patterns. They change only
the order of rows and the direction of continuous strokes, never which
strokes exist; the retained LINE/NET/DIM comparisons are unchanged and
still pass. Native policy on top of the evidence: a start quotient within
1e-9 (relative) of an integer is that integer, so rotation noise cannot move
the start row. Because the original's orientation rule is not rotation
invariant, the Rust contract that a rotated boundary gives rotated strokes
compares strokes in order but in either direction
(`hatch_continuous.rs::pattern_origins_rotate_and_scale_with_the_boundary`),
and the PLAST/PLASTI and TRANS/INSUL row-order expectations were corrected
to the original's order (each is compared directly).

The runner cannot execute the original's hatch for patterns whose rows have
a non-zero delta-x (for example `DASH`, `DOTS`, `EARTH`); it stops on an
unsupported DOS service. Those are therefore not oracle anchors here.

The help file and the runner do not show how the original handles patterns
too large for memory, malformed `ACAD.PAT` lines or pattern files other than
`ACAD.PAT`. Everything about external files below is native policy.

## Native contract

### U prompts

```
HATCH: pattern (name,style / U / ?)      U[,N|O|I]
HATCH: angle for crosshatch lines        number, or a point (then)
HATCH: angle second point                point; angle = direction first->second
HATCH: spacing between lines             number, or a point (then)
HATCH: spacing second point              point; spacing = distance
HATCH: double hatch area (Y/N) <N>       Y | YES | N | NO | Return (= N)
HATCH: select objects on Window or Last  (as for named patterns)
```

- Replies are case-insensitive; `U` takes the same `,N`/`,O`/`,I` suffix as
  a named pattern and an invalid suffix returns to `Command` like a named
  pattern does.
- Angle: any finite number (degrees, normalized like the named-pattern
  angle). Return, non-numbers and non-finite values are rejected and the
  prompt stays (native retry instead of the original's `*Invalid*` exit).
  The second point must be finite and differ from the first.
- Spacing must be positive and finite. Return, `0`, negative, non-numeric
  and non-finite answers, and a zero-length point pair, are rejected; the
  spacing prompt stays (a second-point failure returns to the spacing
  prompt). The original instead accepts `0`/negative and silently creates
  nothing; native rejects early so no prompt sequence ends without output.
- Double: other answers are rejected with the prompt kept (the original
  treats them as `N`).
- Mouse picks are accepted at the angle/spacing prompts as points.
- Geometry: `U` is exactly the pattern `{angle 0, origin (0,0), delta-x 0,
  delta-y spacing}` (plus `{angle 90, ...}` when double) at scale 1 and the
  user angle, run through the existing sweep: centre-out row order, island
  styles, the aggregate 100,000-stroke budget, the `*Xn` block on layer 127,
  one INSERT on the current layer and one undo snapshot. `U,angle,s` equals
  `LINE` at scale `s/0.125`; double equals `NET`.
- Errors (budget, boundary) leave the drawing, undo history and dirty state
  unchanged and keep the selection prompt. Cancel at any U prompt leaves no
  block, INSERT or undo step.

### External pattern files

A pattern name that is not `U`, `?` or one of the 23 built-in names is a
pattern-file request. Built-in names always use the built-in geometry and
never read a file (the existing catalogue is kept as is). The editor stays
filesystem-free: `Editor::hatch_pattern_file_request` reports the request,
the Session reads the file and hands its bytes to
`Editor::submit_hatch_pattern_file`. Return, screen-menu GO, menu macro
pieces, the API and MCP all take this route. An `Editor` without a Session
still reports `unknown HATCH pattern: NAME`.

Lookup (Session), reusing the INSERT file resolver
(`docs/native-external-insert.md`):

1. The reply before the style comma, in its typed case, is the file
   specification. A path containing a comma is not supported.
2. `D:rest` uses the FILES drive mapping; an absolute path is used as
   given; a relative path is tried in the attached document's directory,
   then the process directory. Without an extension `NAME.PAT` then
   `NAME.pat` are tried. The pattern looked up inside is the file's base
   name without directories or extension, upper-cased.
3. For a bare name only (no directory, drive or extension), when no
   `NAME.PAT` exists, `ACAD.PAT` then `acad.pat` are tried in the same
   directories; this mirrors the original, which reads every named pattern
   from `ACAD.PAT`.
4. Only regular files (or symlinks to them) are candidates; directories,
   FIFOs and devices are skipped and the search continues. If nothing is
   found the reply is an unknown pattern as before.
5. The read is capped at 262,144 bytes (about 51 times the retained 5,120-byte
   `ACAD.PAT`); a larger file, or one growing past the cap while read, is
   refused. The editor enforces the same cap on bytes handed to it.

Parsing (editor, `crates/acad-cmd/src/hatch_pattern.rs`), in the retained
`ACAD.PAT` syntax:

- The text ends at the first DOS end-of-file byte (0x1A) or the end of the
  data; the retained file has stale bytes after its 0x1A. Lines end with LF
  or CRLF; at most 8,192 lines are scanned.
- `*name[,description]` starts a definition; the first header whose name
  equals the requested name ignoring ASCII case is used. Its rows run to the
  next `*` line. Blank lines and lines starting with `;` are skipped
  (native extension; the retained file has none).
- A row is `angle, x-origin, y-origin, delta-x, delta-y [, dash ...]`:
  comma-separated decimal numbers, whitespace around fields ignored. All
  values must be finite; `delta-y` (row spacing) must be positive; at most
  16 dash entries, positive for strokes, negative for gaps, zero for dots;
  a dash list needs at least one stroke or dot (all zeros, or gaps only, is
  an error; no dash list at all is a continuous row). A definition has 1 to 64 rows; non-ASCII rows are errors.
- Total work is bounded: every sweep row and every dash cycle visited, over
  all families, is charged before it runs against a limit of 10,000,000;
  past it HATCH fails (`HATCH work exceeds the limit of 10000000 rows and
  dash cycles`) with the drawing, undo history and dirty state unchanged and
  the selection prompt kept. This stops rows whose strokes are too short to
  be visible at their coordinates, which never reach the 100,000-stroke
  budget. Built-in patterns are far below it whenever they fit the stroke
  budget.
- Boundary-edge work is bounded separately from that charge: the selected
  loops are chained once per HATCH (not per family), and each family's rows
  sweep an active set of edges and circles indexed by their offset range
  across the rows, so a row examines only the boundary items it can cross
  instead of every edge. Output is identical to testing every edge. A
  2,000-square boundary (8,000 edges) with 21 continuous families of about
  100,000 rows each takes about 0.05 s in a release build (20 s before the
  index) and stays under 10 s in debug (regression in `hatch_user.rs`; a
  unit test bounds the edge visits).
- Only the requested definition is validated. Errors name the source and
  the 1-based line, for example `HATCH: ACAD.PAT line 7: delta-y must be
  positive`. A missing definition is `HATCH: FOO.PAT has no *FOO
  definition`.
- Rows map to the same families as the built-in catalogue (angle, origin,
  delta-x as row drift, delta-y as spacing, dashes); parsing the retained
  `ACAD.PAT` reproduces all 23 built-in definitions exactly.
- File errors (unreadable, over the cap, parse errors, missing definition)
  return to `Command` like an unknown name, with nothing changed. After a
  file pattern is accepted the usual scale and angle prompts follow, and the
  style suffix applies.

## Contracts

- `crates/acad-cmd/tests/hatch_user.rs`: prompts, two-point answers,
  retries, `U` = scaled `LINE`/`NET`, U with N/O/I on nested islands,
  invalid suffix, budget exhaustion atomic with retry and cancel, cancel at
  each prompt, one UNDO, file request routing, parse errors with line
  numbers and limits, EOF/comments/case, retained-`ACAD.PAT` parity with the
  23 built-ins (skipped visibly without the corpus), dense-boundary
  responsiveness in all three styles.
- `crates/acad-app/tests/hatch_user.rs`: Session lookup order (document
  directory, process directory via absolute paths, `NAME.PAT` before
  `ACAD.PAT`, explicit extensions), directories skipped, missing and
  over-cap files, API and MCP routes, `U`/file hatches through one UNDO and
  DWG/DXF save/open round trips.
- `crates/acad-oracle/tests/hatch_user.rs`: the original facts above.

The retained LINE/NET/DIM oracle comparisons keep their strict tolerances.
