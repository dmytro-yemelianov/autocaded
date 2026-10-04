# SKETCH: original evidence and native policy

Ledger row K (`docs/superpowers/plans/2026-10-04-agentic-contracts.md`). The
previous native editor accepted a positive record increment and then refused
with "SKETCH requires a digitizer input device". `ACAD.HLP` lines 566–581
(`crates/acad-cmd/resources/acad.hlp`) say SKETCH "requires a pointing device
such as a digitizing tablet, mouse, or light pen", so a mouse is a documented
input device and the refusal was not a completed command.

## Evidence

Two sources only:

1. **HLP** (lines 566–581): the `Record increment:` value, the control prompt
   `Sketch.  Pen eXit Quit Record Erase Connect .` and the seven subcommands
   P (raise/lower pen), X (record and exit), Q (discard temporary lines and
   exit), R (record and remain), E (erase temporary lines from a specified
   point to the end), C (connect: restart at the last end point) and
   `.` (line from the end to the current point, pen up).
2. **QEMU oracle** (`crates/acad-oracle/tests/sketch_mouse.rs`). The original
   ACAD.EXE runs under QEMU with its configured Mouse Systems serial mouse
   (DGMS on COM1). The test moves the mouse, presses the left button and
   types control keys, reads the graphics-mode command lines by matching the
   BIOS 8×8 font, and parses the saved DWG. Every "observed" row below
   is asserted there. The test skips visibly when System.img or
   `qemu-system-i386` is absent.

| # | Observed in the original (oracle test) |
|---|---|
| O1 | After the increment the prompt is `Sketch.  Pen eXit Quit Record Erase Connect .` |
| O2 | A left-button click toggles the pen, just as the P key does |
| O3 | With the pen down, a vertex is recorded only when the pointer is at least one increment from the last vertex **on one axis** (Chebyshev distance, world units). With increment 1, a diagonal move of 0.83 × 0.88 (Euclidean 1.21) records no vertex, while a 1.18 vertical move does |
| O4 | Raising the pen (click or P) records the remaining segment to the exact pointer point, even when it is shorter than the increment (asserted with a 0.245 move at increment 1) |
| O5 | Consecutive collinear segments in the same direction become one LINE (axis-aligned case observed). Segments in opposite directions stay separate |
| O6 | R with the pen down also records the tail to the current point, and drawing continues from it. R and X report `N lines recorded.`; X then returns to `Command:` |
| O7 | Q exits and discards only temporary lines. Lines recorded earlier with R stay in the DWG |
| O8 | Ctrl-C during SKETCH also discards temporary lines and keeps recorded ones |
| O9 | `.` with the pen up adds a line from the last end point to the pointer and leaves the pen up. With the pen down, `.` does nothing |
| O10 | C prints `Connect:  Move to endpoint of line.` and lowers the pen at the exact last end point once the pointer is within the increment (0.196 connects and 0.294 does not, increment 0.25). It connects at once if the pointer is already there |
| O11 | C with no last point prints `No last point known.` / `Connect aborted.`. C with the pen down prints `Connect command meaningless when pen down.` / `Connect aborted.` |
| O12 | E prints `Erase:  Select end of delete.`. P then cuts the path back through the temporary vertex nearest the pointer: the first line with that end point and every later line are erased. A pointer next to the stroke's start erases everything. With the pointer one pixel from a shared vertex, and nearer the following line's interior, the line ending at the vertex is still erased, so the rule is nearest vertex, not nearest line. E again prints `Erase aborted.` and keeps every line |
| O13 | SNAP applies to sketched points. ORTHO keeps every recorded line horizontal or vertical from the previous vertex. The pen-down tail recorded by a pen-up, X or R is an L: a leg along the larger displacement from the last vertex (both the vertical-first and the horizontal-first case are asserted), then the other leg to the pointer itself. After R, sketching continues from the pointer, not from the corner |
| O14 | Return at the sketch prompt behaves as X |

Original points are crosshair pixels on the 640×200 CGA screen. A snapped
point is quantized back to a pixel, so the oracle asserts coordinates to one
CGA pixel: 0.0246 world units in x and 0.0589 in y at the default view. Pixel
quantization is not reproduced (see divergences).

## Native contract

**Input model.** The native app has no digitizer. The pointer is the
mouse, the window cursor or the API/MCP `motion` route. The left mouse button,
or a point given through API `point`/`click`, is the pen toggle (O2). The
keyboard letters P X Q R E C `.` are case-insensitive. A typed letter takes
effect at once in the GUI, without Return. Through the API it is one
`command` input. Return (empty input) is X (O14). Whitespace typed in the GUI
is ignored while sketching. Any other input is rejected and the state is kept.

**Sampling.** Every pointer-motion event is one sample. The threshold is in
world units: a sample becomes a vertex when
`max(|dx|, |dy|) >= increment` from the last vertex (O3). There is no separate
screen-pixel threshold. Native samples arrive at window-pixel resolution, so the
effective floor is one screen pixel of the current view. The increment must be
positive and finite.

**SNAP/ORTHO.** These use the same rules as other mouse points
(`mouse_input.rs`). SNAP snaps every sampled point to the world-origin grid.
While the pen is down, ORTHO keeps each new vertex horizontal or vertical from
the last vertex, and the larger displacement wins (O13). A pen-up (click or P),
R and X with the pen down all record the observed L-shaped tail: the ORTHO
point, then the snapped pointer. The last end point becomes the snapped
pointer, so R continues from it (O13). E with the pen down records the same L
(native policy). Native policy (not observed): pen-up motion and `.` use SNAP
only. The crosshair previews the ORTHO point. The rubber band shows both legs
of the tail that would be recorded.

**Temporary path.** Vertices form temporary segments held in the editor's
SKETCH state. They are not part of the drawing, entity count, dirty state,
DXF/DWG output or saves until R or X. The shared frame draws them in cyan, the
erase-pending suffix in red, and the pen-down rubber legs (the tail that
would be recorded now) in cyan. Collinear same-direction segments merge (O5, exact float test). Native
policy: the merge test only needs the new segment to start at the previous
segment's end. A new stroke that begins exactly there (pen up then down at the
same point, a `.` line, a connect) therefore also merges into it.

**Controls.**

| Key | Native behaviour |
|---|---|
| P / click | Pen up → down at the constrained pointer, starting a new stroke. Down → up after recording the tail (O4, O13). Erase mode: confirm (O12). Connect mode: P aborts connect. A click first moves the pointer, so a click within the connect tolerance connects and then lifts the pen, recording the line from the end point to the click. A click outside the tolerance aborts connect (native policy) |
| R | Record the tail if the pen is down, then record all temporary lines as LINE entities on the current layer. Status `N lines recorded.`; stay in SKETCH (O6) |
| X / Return | R, then exit to `Command` (O6, O14) |
| Q | Discard temporary lines and exit (O7) |
| E | Raise the pen, recording the tail (native policy), then preview erasure from the first line with an end point nearest the pointer (Euclidean; earliest line on ties). P confirms (O12) and E aborts. With no temporary lines: `No temporary lines to erase.` (native) |
| C | O10/O11 messages and behaviour. Native policy: the tolerance is Chebyshev ≤ increment (only an axis-aligned approach was observed), and C in connect mode aborts |
| . | O9 |
| Esc / Ctrl+C / Cancel / menu ^C | Discard temporary lines and return to `Command` (O8 observed Ctrl-C) |
| Other Ctrl/Alt/Cmd chords | Ignored while sketching (GUI): a chord never acts as an immediate control, so Ctrl+Q does not quit |
| Window close / API `quit` | QUIT confirmation. The SKETCH state, temporary lines included, is suspended and restored if the answer is not Y/YES. Cancelling the confirmation discards it (native policy) |

In erase or connect mode, any other control first aborts that mode (printing
the "aborted" message) and is then processed normally. This is native
policy; the original was not observed.

After an erase, the last end point becomes the start of the first erased line
and the pen is up (native). The last end point persists across R within one
SKETCH command.

**Bound.** At most 10 000 temporary segments
(`MAX_SKETCH_SEGMENTS`). Once full, samples add no segment and the status asks
the user to record (R), exit (X) or quit (Q). R and X never drop the pen-down
tail: it joins the recorded batch, which can then hold one more line than the
bound. A pen-up with a full buffer cannot keep its tail; it lifts the pen and
shows the buffer-full status. R empties the buffer.

**UNDO.** Each R/X that records at least one line takes one undo snapshot, so
one `UNDO` after SKETCH removes exactly the last record batch. A record of
zero lines takes no snapshot.

**Persistence.** Recorded lines are ordinary `LINE` entities. They round-trip
through DWG and DXF save/open like typed LINEs.

**Shared event flow.** `Editor::pointer_moved(world)` is the one sampling seam.
`Session::pointer_motion` maps the stored window cursor through the frame
viewport and ignores the menu panel and command area. The GUI `CursorMoved`
handler, the API `motion {x, y, width?, height?}` request and the MCP
`acad_motion` tool all call it. Clicks, keys and Return use the existing
`click`/`point`/`command` routes. `state.sketch` reports pen, mode and
temporary count.

## Divergences and limits

- Points are not re-quantized to CGA pixels. A snapped native point is exact.
- The original samples a moving pointer at its own polling rate. The native
  editor samples once per delivered motion event. Fast moves can therefore
  give different intermediate vertices, while the rules above stay the same.
- Collinearity merging is proven only for axis-aligned runs. The native editor
  merges any exactly collinear, same-direction pair.
- Unobserved behaviour is native policy and is marked as such above: keys in
  erase/connect modes other than P/E/C, E with no temporary lines or with the
  pen down, the connect metric, merging across strokes,
  ORTHO with the pen up, modifier chords, quit suspension, the segment bound
  and UNDO granularity (the original 1.4 has no UNDO).
- The record increment has no remembered default.
