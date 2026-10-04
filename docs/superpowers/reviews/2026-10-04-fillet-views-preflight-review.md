# R4 V1 source review — pass 1

Candidate: `v1.patch`, SHA256 `1a782fc794676211102ce87bb8860d41434abd823aebaaa950fa1204431bb1b0`, baseline `c9657fbb93960051007f5a248daf143e51fa640d`; staged private workspace `fillet-views`. Exact exported/staged identity checked. Source-only review; worker test results are not independent execution evidence. Verdict: **needs fixes**.

## P2 — finite input can commit an unusable viewport

`crates/acad-cmd/src/view.rs:57–77`, reached by `commit_view` at 123–127, validates positive mathematical half-spans and finite computed corners but neither representable corner separation nor finite rendering scale. `crates/acad-render/src/viewport.rs:15` computes `pixel_height / height`, and `to_screen` at 43 multiplies the coordinate difference by that scale.

Minimal case A: submit `ZOOM`, `C`, `1e308,1e308`, `1`. Both x corners and both y corners round to the same center, yet validation succeeds and changes drawing/Previous. Screen clicks map to that same world point. Minimal case B with normal positive pixel canvas: `ZOOM`, `C`, `0,0`, `1e-308` passes the half-span checks, while a 556-pixel canvas gives infinite scale; rendering the center computes `0 * infinity`, i.e. NaN. These are source/arithmetic counterexamples, not claimed executed application reproductions.

Require strictly ordered representable x/y corners and finite positive render scale for the relevant pixel canvas before committing any view. The editor currently remembers aspect only; preserve sufficient canvas information or use an explicit conservative representability policy. Apply the shared guard to C/L/W/E/A, relative/absolute factors, P and PAN commits. Add regression assertions that errors retain current view, Previous, prompt/retry state, and dirty baseline; test both coordinate-collapse and tiny-height cases, including the application viewport seam. This closes the declared positive finite view boundary rather than altering measured All constants.

## Reviewed without further source findings

- FILLET R sets finite nonnegative drawing state, has no-op/undo behavior, and selection-to-R discards only pending selection. Geometry is calculated before the two source-line mutations and optional current-layer arc commit. Zero radius intersection, external endpoint retention, near-parallel/degenerate/excessive positive radius and representability checks follow the documented native policy.
- The existing positive crossing-lines geometry expectations retain their precision. New acute/right/obtuse tests inspect tangency/radius rather than only entity count. Generalized digitizer-side parity is not claimed.
- AC1.40 parse/write maps radius to independently audited offset `0x1fa`, defaults to zero for AC1.2, and overwrites the known slot even with passthrough header bytes. Both public/low-level checked DWG paths reject invalid radius and nonzero AC1.2. Historical DXF uses checked nonzero refusal without an invented token; Drawing API reaches that checked serializer. Existing application save staging preserves destination/attachment/baseline on errors; tests inspect those invariants, END original revision, WBLOCK and reopen. Original eight AC1.40 sample defaults are checked separately from synthetic nonzero bytes.
- ViewInput commits only on completed valid input; Previous toggles through existing set_view behavior. Bare numeric factors derive from All and X derives from current view. PAN subtracts displacement consistently with moving geometry from first screen point to second. W/E/L use configured physical drawing-canvas aspect, and mouse navigation bypasses SNAP/ORTHO.
- Visible bounds share a global stored-visit budget across owners and compact REPEAT/rotated INSERT geometry, preserve layer filtering, and reject exhausted/nonfinite traversal. The measured All constant/anchor remains unchanged; conservative text/shape/arc bounds and standalone aspect are explicitly documented limitations.
- Policy distinguishes retained help/static field evidence, measured old tests, synthetic persistence checks, and new Rust interaction/geometry choices. No original nonzero FILLET export or generalized parity is asserted.

## Next gate

Review a repaired exact patch for the viewport finding (pass 2), then inspect root's final integrated candidate including B4 match exhaustiveness/erased-member traversal and the GUI harness delta. V1 and R4 are not implementation-complete from this review.
