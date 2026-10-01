# Screen Menu Rendering & Mouse Dispatch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render AutoCAD 1.4's parsed screen menu (`.MNU`) as a clickable on-screen panel in `acad-app`, and wire a mouse click on a plain-text menu entry to submit that entry's macro through the same command dispatcher keyboard input already uses — closing `docs/HANDOVER-2026-09-30.md`'s "Known limits and next work" item 3 for the single-page, plain-text-entry case.

**Architecture:** `MENU`/`Effect::LoadMenu` already parses `.MNU` into a flat `Vec<MenuEntry>` (`crates/acad-cmd/src/menu.rs`), but nothing renders it and nothing dispatches clicks — `acad-app`'s window is currently one edge-to-edge drawing canvas with no reserved panel region (`crates/acad-app/src/main.rs:277-320`). This plan adds a panel region, a hand-rolled bitmap-font renderer for its labels (not a reuse of `acad-render`'s SHP-font `Text` pipeline — see Task 2's rationale), and a mouse-hit-test branch ahead of the existing `accepts_mouse_point`/`accepts_mouse_selection` dispatch in the `MouseInput` handler (`main.rs:258-276`).

This plan is evidence-first, matching this project's established recovery discipline (see `docs/superpowers/plans/2026-09-30-dim-angular-recovery.md` for the pattern this follows). AutoCAD 1.4's screen menu auto-paginates a flat entry list into fixed-size pages with a synthetic `NEXT` control — this pagination is **not** encoded anywhere in the `.MNU` file format or in `crates/acad-cmd/src/menu.rs`'s parser; it is the native screen-menu renderer's own runtime behavior, and this codebase has not recovered it yet. **Task 1 recovers it under QEMU before any rendering/pagination code is written.** Everything downstream of Task 1 that depends on its exact numbers (items per page, `NEXT`'s and `< GO >`'s click behavior) is deliberately left unspecified in this document until Task 1's findings exist, exactly as the DIM plan deferred its Tasks 2+.

**Tech Stack:** Rust 2021, `winit`/`softbuffer` (already used by `acad-app`), the existing `acad-oracle` QEMU harness (`Session::mouse_to`/`mouse_pin`, `crates/acad-oracle/src/mouse.rs`) for the recovery and differential-test tasks.

**Spec:** `docs/HANDOVER-2026-09-30.md` ("Known limits and next work", item 3) and the already-parsed `corpus/System/ACAD.MNU` (57 entries, confirmed by `cat -A`/`nl` during planning — see Global Constraints for the exact entry inventory).

## Global Constraints

- No behavior change to anything already working: typed commands, mouse point-entry (`submit_mouse_point`), and mouse entity-selection (`pick_mouse_entity`) must keep working exactly as before everywhere outside the new panel's screen rectangle.
- `corpus/System/ACAD.MNU` has exactly 57 entries, confirmed during planning (`nl -ba corpus/System/ACAD.MNU`): one `Header`-kind entry (`[< GO >];`, action is a single `;` byte, at the very top) and 56 `Item`-kind entries. Three entries carry a single control byte as their action rather than printable macro text: `^Snap` (`0x02`), `^Ortho` (`0x0f`), and three separate `^Cancel` entries (`0x03`) at lines 20, 38, 57 of the file. Every other entry's `action` is plain ASCII macro text (e.g. `LINE`, `zoom a`, `end;`, `quit`) that can be `str::from_utf8`'d and passed straight to `Editor::submit` unchanged.
- **Control-byte entries (`^Snap`, `^Ortho`, `^Cancel`) are out of scope for dispatch in this plan.** Their native behavior when *clicked* (an instant toggle? a full `SNAP`/`ORTHO` command invocation requiring further input?) has not been recovered under QEMU and must not be guessed — `crates/acad-cmd/src/dispatch.rs` already has real `"SNAP" | "RES" | "RESOLUTION"` and `"ORTHO"` text commands (`dispatch.rs:1236,1246`) that expect further state-machine input, and nothing establishes that a raw `0x02`/`0x0f` keystroke maps onto those same code paths rather than a different instant-toggle behavior. This plan renders these three entries in the panel (so the menu looks complete and matches native) but clicking them is a documented no-op with a status-bar message, not a guess at their real effect.
- `acad-render`'s only text-rendering path (`Library::text()`, `crates/acad-render/src/flatten.rs:297-316`) needs a loaded SHP font library and operates in world-space `Entity::Text` coordinates — it is not available or appropriate for fixed-size screen-space UI chrome that must render correctly even when no font library has been loaded yet (the native screen menu is always visible from startup, independent of `LOAD`). This plan adds a small, self-contained embedded bitmap font for panel labels instead (Task 2) — this was considered and is a deliberate choice, not an oversight.
- `acad-oracle` must never become a dependency of `acad-app` (enforced by `acad-oracle/Cargo.toml`'s own comment: "this crate must not be a dependency of the shipped application"). All QEMU/mouse-harness work stays in `crates/acad-oracle`'s own tests/examples.
- Every modified/new file must pass the existing gates: `cargo test -p acad-app -p acad-cmd`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`.

## Review Focus

- **Clicking in the panel region while a command is mid-prompt expecting a point** (e.g. `LINE` awaiting its first point): a reasonable user expects clicking a menu item to behave like typing that macro at the current prompt (cancelling/redirecting as the macro's own text would, e.g. if the macro starts with a cancel), not for the click to be silently swallowed or misrouted to `submit_mouse_point` as if it were a drawing-canvas click. Task 3's dispatch must check the panel hit-test *before* the existing `accepts_mouse_point`/`accepts_mouse_selection` branches, not after.
- **Clicking outside any entry's row within the panel's rectangle** (e.g. in the gap between the title bar and the first entry, or below the last visible entry on a short page): must be a no-op, not a crash or a misattributed click on the nearest entry.
- **Window resize while the menu panel is visible**: the panel's screen rectangle is currently proposed as a fixed-width right-hand column — Task 2/3 must recompute its rectangle from the live window size every frame (mirroring how `viewport_for` already recomputes from `size.width`/`size.height` each `RedrawRequested`), not cache stale pixel coordinates from a previous frame.
- **No menu loaded yet** (`self.menu: Option<MenuFile>` is `None` until `MENU` is run): the panel must not render at all, and the mouse hit-test must not treat empty window space as panel space — this is the default state every session starts in, not an edge case to patch in later.
- **A macro whose decoded text contains a trailing newline-equivalent or multiple commands** (e.g. `end;` is `"end;"` as one string — AutoCAD macros use `;` as an Enter-equivalent separator within one macro, not just a literal semicolon character): verify whether `Editor::submit` already treats embedded `;` as a line separator before assuming `str::from_utf8(action).unwrap()` fed whole to one `submit` call is correct, or whether the macro needs splitting on `;` into multiple `submit` calls first. This is a concrete decode-correctness question Task 3 must resolve by reading `dispatch.rs`, not assume either way.

---

## Task 1: Recover screen-menu pagination behavior under QEMU

**Files:**
- Create: `crates/acad-oracle/examples/menu-pagination-recovery.rs`
- Test: none (a recovery tool, following the exact pattern of `crates/acad-oracle/examples/dim-angular-recovery.rs` — not a `#[test]`, its job is to capture and save screens for a human/agent to read, not to assert anything yet)

**Interfaces:**
- Consumes: `acad_oracle::session::Session::boot_disposable`, `Session::type_line`, `Session::wait_for_text`, `Session::wait_until_text_gone`, `Session::capture_editor`, `Session::mouse_pin`, `Session::mouse_to` (all in `crates/acad-oracle/src/session.rs`); `acad_oracle::mouse::device_for_pixel` (used exactly this way in `crates/acad-oracle/tests/fillet_mouse.rs:84`); `acad_oracle::cga::Frame::new`/`to_rgb_640x400` (in `crates/acad-oracle/src/cga.rs`); `tiny_skia::Pixmap`/`PremultipliedColorU8` for PNG encoding (already a dev-dependency of `acad-oracle` as of the DIM recovery work — reuse the same `save_png` helper shape from `dim-angular-recovery.rs`, don't reinvent it).
- Produces: PNG screenshots under the OS temp dir, plus printed findings — no library code, no production behavior. This task's real deliverable is a findings write-up appended to this plan file's Step 3 (see below), the same pattern Task 1 of the DIM plan used.

- [x] **Step 1: Boot a drawing and capture the default screen-menu page**

Write `crates/acad-oracle/examples/menu-pagination-recovery.rs` with a `main()` that:
1. Resolves `corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img` relative to `env!("CARGO_MANIFEST_DIR")`, self-skipping (`eprintln!` + `return`) if the disk or `acad_oracle::available()` is absent — identical guard to every existing oracle test/example.
2. Boots via `Session::boot_disposable(&disk, None, &[])`, waits for `"Enter selection:"`, types `"1"`, waits for `"Enter NAME of drawing:"`, types a drawing name (`"MENUPAGE"`), waits until that prompt is gone, sleeps 500ms — identical boot sequence to `dim-angular-recovery.rs`'s `capture_each_step`.
3. Calls `vm.capture_editor()` once immediately (no commands typed yet) and saves it as a PNG (`dim-angular-{name}-00-initial.png`-style naming) — this captures the default screen-menu page that should already match the 57-entry file's first ~19-20 items, visible in every screenshot from the DIM recovery work.

- [x] **Step 2: Click the `NEXT` slot and capture what replaces the panel**

Using `vm.mouse_pin()` then `acad_oracle::mouse::device_for_pixel(column, row)` + `vm.mouse_to(x, y, 0)` (move) + `vm.mouse_to(x, y, acad_oracle::mouse::LEFT)` (press) + `vm.mouse_to(x, y, 0)` (release), each separated by `capture_editor()`/a short sleep exactly as `fillet_mouse.rs:84-90` does — click the pixel coordinates where `NEXT` appears in the Step 1 screenshot (read the coordinates off that PNG by eye; the panel occupies roughly the right-hand ~60 columns of the 640-wide frame based on every DIM-recovery screenshot). Capture and save the resulting screen.

Also click the `< GO >` slot (top of the panel) the same way, from the default page, and capture the result separately.

- [x] **Step 3: Record the findings in this plan file**

Captured under QEMU with `cargo run -p acad-oracle --example menu-pagination-recovery` (PNGs saved to the OS temp dir, named `menu-pagination-*.png`, and read by eye plus a small Python/PIL pixel-band scan to get exact row boundaries — eyeballing alone missed a 2-row offset that the pixel scan caught). The native build is the DOS 5.25" AutoCAD 1.4 at `corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img`; `ACAD.MNU` is the exact 57-line/57-entry file at `corpus/System/ACAD.MNU`.

**Pagination is driven by the file's own `*` (repeat) prefix, not a fixed entry count per page.** `ACAD.MNU` has two `*`-prefixed lines: `*POINT` (line 21) and `*LIMITS` (line 39) — already parsed by `crates/acad-cmd/src/menu.rs` as the `repeat` flag ("a leading `*` on an unlabelled command repeats the command until cancel"), confirmed by its own test (`point.repeat == true`). This recovery found that, on the *native screen-menu renderer*, a `*`-prefixed entry **also** forces the start of a new screen-menu page: page 1 runs lines 1-20 (the `[< GO >];` header plus 19 items, ending at `[^Cancel]`), page 2 runs lines 21-38 (18 items, starting at `*POINT` and ending at `[^Cancel]`), and page 3 runs lines 39-57 (19 items, starting at `*LIMITS` and ending at `[^Cancel]`) — 19+18+19 = 56, matching the file's 56 `Item`-kind entries exactly. If pagination were a naive fixed-size chunk (e.g. "next 19 after the previous page"), page 2 would have swallowed line 39 (`*LIMITS`) too, filling it to 19 items; it does not — it stops one short, exactly at the line before the next `*`-entry. This is strong, direct evidence of a forced page break on `*`, not a coincidence of page capacity. **This is a correction to this plan's own Spec/Architecture section's assumption** ("AutoCAD 1.4's screen menu auto-paginates a flat entry list into fixed-size pages") — the pagination is file-content-driven, not a pure fixed-size auto-chunk; Task 3 (or a later plan revision) needs to honor `*`-entries (and more generally, any future `*`-entry anywhere in a loaded `.MNU`) as page-break points in its page-building logic, not just as the existing `repeat`-command macro-execution semantic.

**Entries visible per page (excluding the `< GO >` header on page 1 and the synthetic `NEXT` control row on every page):**
- Page 1 (default, `00-initial.png`): 19 items — `^Snap`, `^Ortho`, `LINE`, `ARC`, `CIRCLE`, `TEXT`, `INSERT`, `MOVE`, `L`, `W`, `COPY`, `CHANGE`, `FILLET`, `ERASE`, `ERASE L`, `OOPS`, `BREAK`, `REDRAW`, `^Cancel` — plus the `< GO >` header above them and `NEXT` below.
- Page 2 (`01-after-next-click.png`, after one `NEXT` click): 18 items — `POINT`, `TRACE`, `SOLID`, `ARRAY`, `HATCH`, `SKETCH`, `DIM`, `ZOOM All`, `ZOOM Win`, `ZOOM Pre`, `LIST`, `DIST`, `AREA`, `STATUS`, `TABLET`, `FILES`, `PLOT`, `^Cancel` — no `< GO >` header row; the two rows that row would otherwise occupy are left blank at the top of the panel instead.
- Page 3 (`02-after-second-next-click.png`, after two `NEXT` clicks): 19 items — `LIMITS`, `GRID`, `SNAP`, `FILL`, `ON`, `OFF`, `UNITS`, `AXIS`, `BASE`, `BLOCK`, `WBLOCK`, `LOAD`, `LAYER`, `COLOR`, `MENU`, `HELP`, `END/save`, `QUIT`, `^Cancel` — again no header row, one blank row at the top instead.
- A third `NEXT` click (`03-after-third-next-click.png`) reproduced page 1 byte-for-byte (same `< GO >` header, same 19 items in the same order) — confirming `NEXT` **wraps back to page 1** after the last page rather than becoming a no-op. So the file's 57 entries need **exactly two `NEXT` clicks** to see every item once, and a third returns to the start — a 3-page cycle, not an unbounded/no-op tail.

**Panel is bottom-anchored, not top-anchored.** Reading exact row boundaries off the PNGs (a Python/PIL scan of lit-pixel bands in the panel's text columns confirmed this — it is not just an eyeball impression): every page's entries end flush against `NEXT`'s fixed row with no gap (`^Cancel` is always the item immediately above `NEXT` on every page), and any page with fewer than the 19-item maximum leaves its *unused rows blank at the top of the panel*, not at the bottom. Page 1 (20 content rows: header + 19 items) fills rows 0-19 with no blank rows. Page 2 (18 items, no header) leaves rows 0-1 blank and starts `POINT` at row 2. Page 3 (19 items, no header) leaves row 0 blank and starts `LIMITS` at row 1. (Confirmed by cropping and 4x-upscaling the panel's top-left corner for pages 2 and 3 and reading the blank space directly — see `/tmp/crop-01-after-next-click.png`/`/tmp/crop-02-after-second-next-click.png` from this run, not committed, but reproducible from the saved full-page PNGs.)

**`< GO >` does not silently no-op and does not (at least from an already-page-1 state) visibly change page.** From page 1 (reached here via the third `NEXT` click's wraparound), clicking `< GO >`'s row/column produced, on the Command: line, `Command:` followed by ` Unknown command. Type ? for list of commands.` and a fresh `Command:` prompt (`04-after-go-click.png`) — the panel itself stayed on page 1, unchanged. This is a real, reproducible observation, not a click miss: the same pixel-to-device coordinate mapping (`acad_oracle::mouse::device_for_pixel`) correctly and repeatably hit `NEXT`'s row three times in a row to produce the exact expected page sequence above, so there is no reason to doubt it also hit `< GO >`'s row/column correctly. What is **not** fully resolved: *why* a header whose action is confirmed (by `crates/acad-corpus`'s classification and this plan's own file inspection) to be a bare `;` byte produces an "Unknown command" error rather than simply re-displaying `Command:` the way a bare Enter normally would at that prompt. One plausible explanation consistent with everything observed: the native digitizer-menu dispatch path sends the picked screen coordinates as literal text to the command buffer in addition to (or instead of) the mapped macro action when no command is currently active, and an unrecognized numeric/coordinate string at a bare `Command:` prompt is exactly what produces this generic AutoCAD error — but this recovery did not isolate that mechanism further (e.g. by comparing against a literal keyboard-typed `;` at the same prompt), and that is explicitly left as a further-work item rather than guessed at. Task 3 should treat `< GO >`'s click behavior as **not yet a confirmed no-op** and should not assume it resets to page 1 from a later page without its own direct probe.

**Clickable-slot geometry (hit-test rectangle), in the 640×400 rendered-frame pixel space `to_rgb_640x400()` produces:**
- Row height: **16 display pixels** per row (8 native CGA scanlines, doubled — confirmed by a lit-pixel band scan of `00-initial.png`: 21 bands at 16px spacing for the header + 19 items + `NEXT`). Row `r` (0-indexed from the top of the panel) occupies display rows `[16r, 16r+15]`.
- Fixed row assignment: row 0 is the header slot (`< GO >` only on page 1; left blank — not reusable by an item — on pages without a header), rows 1-19 are the (up to 19) item slots, and row 20 is always `NEXT`'s fixed slot, regardless of how many items the current page has. A page with fewer than 19 items leaves its unused slots blank at the *top* of the 1-19 item range, immediately below the header row, not at the bottom next to `NEXT`.
- **Important unit mismatch to carry into Task 3**: `acad_oracle::mouse::device_for_pixel(column, row)` — used for all clicks in this recovery, per `fillet_mouse.rs`'s established pattern — takes `row` in **native, undoubled CGA space** (0-199ish), not the doubled 640×400 display space the saved PNGs are in. Concretely: display row `y` (as read off a PNG) corresponds to native row `y / 2`; this recovery's clicks used `native_row = 8 * r + 4` (the middle scanline of row `r`'s 8-native-scanline band) for whichever row `r` it needed to hit. A first attempt at this recovery, written straight off the 640×400 PNG's pixel rows without halving, missed every row it aimed at (all three `NEXT` clicks were silent no-ops) until this halving was applied — worth flagging explicitly since Task 3's own hit-test, if it ever needs to translate a rendered pixel back to a native-space concept, must not repeat that mistake. (This matters only for anything built on `acad_oracle::mouse`'s native-CGA-space API; `acad-app`'s own window/panel rendering and its mouse hit-test are free to work entirely in display/window pixel space, where the 16px-per-row, 640×400 numbers above apply directly, with no halving needed.)
- Column: the panel's left edge is a 4px-wide vertical divider line at display columns 568-571 (fully lit top-to-bottom, confirmed by a per-column lit-pixel count scan); label text begins around column 576 and the panel extends to the frame's right edge at column 639. The clickable column range used for every click in this recovery was column 600 (comfortably inside every label's text, clear of the divider) — this recovery did not probe the exact left/right boundaries of what counts as "inside" a row's clickable slot (e.g. whether column 569 or column 639 still registers), only that the full row (whatever its true horizontal extent) is what dispatches on click, consistent with every `NEXT` click landing correctly at a single fixed test column. Task 3 should treat "anywhere in columns ~572-639 at the entry's row" as the working hit-test rectangle, understanding that its exact left boundary was not separately verified.

- [ ] **Step 4: Commit the recovery tool and findings**

```bash
git add crates/acad-oracle/examples/menu-pagination-recovery.rs docs/superpowers/plans/2026-10-01-screen-menu-rendering.md
git commit -m "research(oracle): recover AutoCAD 1.4 screen-menu pagination behavior"
```

---

## Task 2: Embedded bitmap font and panel-rendering primitive

**Files:**
- Create: `crates/acad-app/src/menu_panel.rs`
- Modify: `crates/acad-app/src/main.rs` (add `mod menu_panel;`)

**Interfaces:**
- Consumes: nothing from Task 1 (this task is pure rendering infrastructure, independent of the recovered pagination numbers — it draws whatever one page's worth of entries it's given).
- Produces: `pub(crate) const GLYPH_WIDTH: usize`, `pub(crate) const GLYPH_HEIGHT: usize` (the embedded font's fixed cell size); `pub(crate) fn draw_text(buffer: &mut [u32], buffer_width: usize, x: usize, y: usize, text: &str, color: u32)` (blits `text` starting at pixel `(x, y)`, one `GLYPH_WIDTH`-wide cell per character, clipping silently at `buffer_width`/buffer length rather than panicking on an out-of-range label); `pub(crate) fn draw_rect(buffer: &mut [u32], buffer_width: usize, buffer_height: usize, x: usize, y: usize, w: usize, h: usize, color: u32)` (fills a solid rectangle, same clipping discipline).

- [ ] **Step 1: Embed a minimal 8x8 bitmap font**

In `crates/acad-app/src/menu_panel.rs`, define a `const FONT: [[u8; 8]; 96]` table covering printable ASCII `0x20..=0x7F` (space through DEL — DEL's row can be all-zero, it never prints), one `[u8; 8]` per glyph where each byte is one row's 8 pixels (bit 7 = leftmost, matching the same bit convention `acad_oracle::cga::Font` already uses, for consistency with this project's existing font-table conventions — though this is a wholly separate, hand-authored table since `acad-app` cannot depend on `acad-oracle`). Only the glyphs this task's test actually exercises need accurate bitmaps to start (digits, uppercase A-Z, space, and the punctuation seen in `ACAD.MNU`'s labels: `< > ^ / : . *`); any glyph left as a blank box is a visible, honest "not drawn yet" rather than a silent wrong shape — do not fabricate pixel data for glyphs you have not actually encoded.

- [ ] **Step 2: Write the test for `draw_text`/`draw_rect`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_rect_fills_only_the_requested_area_and_clips_at_buffer_edges() {
        let (width, height) = (10usize, 10usize);
        let mut buffer = vec![0u32; width * height];
        draw_rect(&mut buffer, width, height, 2, 2, 100, 100, 0xFF0000);
        // Clipped to the buffer: every pixel from (2,2) onward is filled,
        // nothing before row/column 2 is touched.
        assert_eq!(buffer[2 * width + 2], 0xFF0000);
        assert_eq!(buffer[9 * width + 9], 0xFF0000);
        assert_eq!(buffer[1 * width + 1], 0);
        assert_eq!(buffer[0], 0);
    }

    #[test]
    fn draw_text_renders_a_known_glyph_and_stops_at_buffer_width() {
        let (width, height) = (GLYPH_WIDTH * 2, GLYPH_HEIGHT);
        let mut buffer = vec![0u32; width * height];
        draw_text(&mut buffer, width, 0, 0, "AB", 0xFFFFFF);
        // "A" must differ from an untouched buffer (some pixel in its cell is lit).
        let a_cell: Vec<u32> = (0..GLYPH_HEIGHT)
            .flat_map(|row| (0..GLYPH_WIDTH).map(move |col| (row, col)))
            .map(|(row, col)| buffer[row * width + col])
            .collect();
        assert!(a_cell.iter().any(|&p| p == 0xFFFFFF), "expected 'A' to draw at least one lit pixel");
        // A third character past the 2-glyph-wide buffer must not panic or
        // write out of bounds — re-running with "ABC" on the same buffer
        // size must still return normally.
        draw_text(&mut buffer, width, 0, 0, "ABC", 0xFFFFFF);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail, then implement `draw_text`/`draw_rect`/`FONT` to make them pass**

Run: `cargo test -p acad-app menu_panel:: -- --nocapture`
Expected (before implementation exists): compile error, no such function.
Implement `FONT`, `draw_rect`, `draw_text` per Step 1's spec. Each glyph cell is `GLYPH_WIDTH` x `GLYPH_HEIGHT` pixels (suggest 8x8, scaled 2x to 16x16 on screen for legibility at typical window sizes — bake the 2x scale into `draw_text` itself, not into the font table).
Run again: `cargo test -p acad-app menu_panel:: -- --nocapture`
Expected: both tests pass.

- [ ] **Step 4: Gates and commit**

Run: `cargo build -p acad-app`, `cargo test -p acad-app`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`. All four must pass.

```bash
git add crates/acad-app/src/menu_panel.rs crates/acad-app/src/main.rs
git commit -m "feat(app): embedded bitmap font and panel drawing primitives"
```

---

## Task 3: Render the loaded menu as a panel, using Task 1's recovered page size

**Files:**
- Modify: `crates/acad-app/src/menu_panel.rs` (add panel layout/rendering)
- Modify: `crates/acad-app/src/main.rs` (call the new renderer from `RedrawRequested`; add panel state to `App`)

**Interfaces:**
- Consumes: Task 1's recovered page size (call it `N`, a concrete number once Task 1's Step 3 is filled in) and `< GO >`/`NEXT` behavior; Task 2's `draw_rect`/`draw_text`/`GLYPH_WIDTH`/`GLYPH_HEIGHT`; `App.menu: Option<acad_cmd::menu::MenuFile>` (already exists, `main.rs:19`); the `buffer: &mut [u32]`, `size.width`, `size.height` already available at the `RedrawRequested` call site (`main.rs:277-320`, right after the existing `draw_axis_ticks`/`draw_crosshair` calls — append the panel draw there, in the same per-frame pixel-buffer-mutation style).
- Produces: `pub(crate) struct PanelLayout { pub rect: (usize, usize, usize, usize), pub row_height: usize }` and `pub(crate) fn layout_for(menu: &acad_cmd::menu::MenuFile, page: usize, window_width: u32, window_height: u32) -> PanelLayout` (computed fresh every frame, never cached — per this plan's Review Focus on window resize) plus `pub(crate) fn draw_panel(buffer: &mut [u32], buffer_width: u32, buffer_height: u32, menu: &acad_cmd::menu::MenuFile, page: usize, layout: &PanelLayout)`. Adds one new field to `App`: `menu_page: usize` (defaults to `0`, reset to `0` whenever a new `LoadMenu` succeeds in `handle_result`, `main.rs:152-164`).

- [ ] **Step 1: Write the layout test**

```rust
#[test]
fn layout_for_places_n_entries_per_page_starting_at_the_recovered_page_size() {
    // N is Task 1's recovered per-page entry count, substituted for real
    // once Task 1's Step 3 is filled in — this placeholder name must not
    // ship unresolved; it is a forward reference to that task's output,
    // not a literal identifier to compile.
    let menu = acad_cmd::menu::parse_menu(include_bytes!("../../../corpus/System/ACAD.MNU")).unwrap();
    let layout = layout_for(&menu, 0, 640, 480);
    assert_eq!(layout.rect.2 > 0 && layout.rect.3 > 0, true, "panel rect must have positive size");
    // The row height times the recovered per-page count must fit within
    // the panel rect's height for page 0 — pin this to Task 1's N once known.
}
```

(This step's exact assertions depend on Task 1's Step 3 findings for the real per-page count — write the concrete, numeric version of this test once that number exists; do not merge a test that references an unresolved placeholder.)

- [ ] **Step 2: Implement `PanelLayout`/`layout_for`/`draw_panel`**

Lay the panel out as a fixed-width right-hand column (width = longest label's `draw_text` width plus padding, or a fixed value like 10 glyph-cells wide — pick concretely once Task 1's screenshots show the native proportions), one row per visible entry at `GLYPH_HEIGHT`-based `row_height`, a title/`< GO >` row at the top, and (if Task 1 found `NEXT` is a real clickable slot distinct from the 57 `.MNU` entries) a synthetic final row labelled `NEXT` appended by `draw_panel` itself, not stored in `MenuFile`.

- [ ] **Step 3: Wire it into `RedrawRequested` and add `menu_page` to `App`**

In `main.rs`'s `RedrawRequested` arm, after the existing `draw_axis_ticks`/`draw_crosshair` calls (`main.rs:310-316`): `if let Some(menu) = &self.menu { let layout = menu_panel::layout_for(menu, self.menu_page, size.width, size.height); menu_panel::draw_panel(&mut buffer, size.width, size.height, menu, self.menu_page, &layout); }`.

- [ ] **Step 4: Gates and commit**

Run: `cargo build -p acad-app`, `cargo test -p acad-app`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`.

```bash
git add crates/acad-app/src/menu_panel.rs crates/acad-app/src/main.rs
git commit -m "feat(app): render the loaded screen menu as an on-screen panel"
```

---

## Task 4: Dispatch plain-text menu clicks through the existing command path

**Files:**
- Modify: `crates/acad-app/src/main.rs`

**Interfaces:**
- Consumes: Task 3's `PanelLayout`/`layout_for`; `Editor::submit(&mut self, input: &str) -> Result<Effect, String>` (`crates/acad-cmd/src/dispatch.rs:23`, already public); `App::handle_result` (already exists, `main.rs:134-178`).
- Produces: a new `App::handle_panel_click(&mut self, el: &ActiveEventLoop, width: u32, height: u32) -> bool` (returns `true` if the click was inside the panel and handled — including as a documented no-op for a control-byte entry — `false` if the click was outside the panel and the existing `submit_mouse_point`/`pick_mouse_entity` dispatch should run instead).

- [ ] **Step 1: Resolve the embedded-`;`-separator question from this plan's Review Focus**

Before writing `handle_panel_click`, read `crates/acad-cmd/src/dispatch.rs`'s `submit`/`command` to determine whether `Editor::submit` already splits input containing literal `;` characters into multiple logical submissions, or treats `;` as ordinary text within one `&str`. Record the answer as a one-line comment above `handle_panel_click` citing the exact line(s) read, and implement macro decoding accordingly: either split `action` (decoded via `str::from_utf8`) on `;` and call `self.editor.submit(...)` once per piece, or call it once with the whole decoded string — whichever matches what `submit` actually expects, not whichever is simpler to write.

- [ ] **Step 2: Write `App::handle_panel_click`**

```rust
fn handle_panel_click(&mut self, el: &ActiveEventLoop, width: u32, height: u32) -> bool {
    let Some(menu) = self.menu.clone() else { return false };
    let Some((x, y)) = self.cursor else { return false };
    let layout = menu_panel::layout_for(&menu, self.menu_page, width, height);
    let Some(hit) = menu_panel::entry_at(&layout, x, y) else { return false };
    match hit {
        menu_panel::PanelHit::Entry(index) => {
            let entry = &menu.entries[index];
            if entry.action.len() == 1 && entry.action[0] < 0x20 {
                self.status = format!("{} is not yet implemented (control byte 0x{:02x})", entry.label, entry.action[0]);
                return true;
            }
            let Ok(text) = std::str::from_utf8(&entry.action) else {
                self.status = format!("{}: macro is not valid UTF-8", entry.label);
                return true;
            };
            // Decode per Step 1's finding about `;` separators, then submit.
            let result = self.editor.submit(text); // placeholder call shape; adjust per Step 1
            self.handle_result(el, result);
            true
        }
        menu_panel::PanelHit::Next => {
            self.menu_page += 1; // or wrap, per Task 1's recovered NEXT behavior
            true
        }
        menu_panel::PanelHit::Go => {
            self.menu_page = 0; // or whatever Task 1's `< GO >` finding says
            true
        }
    }
}
```

(This step's exact `Next`/`Go` behavior and the `submit` call shape are forward references to Task 1 Step 3 and Task 4 Step 1's findings respectively — fill in the real logic, do not ship the placeholder comments.)

Also add `menu_panel::entry_at(layout: &PanelLayout, x: f64, y: f64) -> Option<PanelHit>` and `pub(crate) enum PanelHit { Entry(usize), Next, Go }` to `menu_panel.rs` in this same task (hit-testing is dispatch logic, test it here alongside `handle_panel_click`, not back in Task 3 which was rendering-only).

- [ ] **Step 3: Write the test for `entry_at`**

```rust
#[test]
fn entry_at_resolves_a_click_inside_a_row_and_none_outside_the_panel() {
    let menu = acad_cmd::menu::parse_menu(include_bytes!("../../../corpus/System/ACAD.MNU")).unwrap();
    let layout = layout_for(&menu, 0, 640, 480);
    // A point inside the panel rect's first entry row resolves to Entry(0)
    // (the second, Item-kind entry — index 0 in the panel's *displayed*
    // list, since `< GO >` at menu.entries[0] is rendered as its own Go
    // slot, not a numbered Entry — confirm this indexing convention
    // matches draw_panel's own row assignment from Task 3).
    let (rect_x, rect_y, _, row_height) = layout.rect;
    let inside = entry_at(&layout, (rect_x + 2) as f64, (rect_y + row_height + 2) as f64);
    assert!(matches!(inside, Some(PanelHit::Entry(_))));
    let outside = entry_at(&layout, 0.0, 0.0);
    assert_eq!(outside, None);
}
```

- [ ] **Step 4: Wire `handle_panel_click` into the `MouseInput` handler**

In `main.rs`'s `MouseInput` left-press arm (`main.rs:262-276`), call `handle_panel_click` first and only fall through to the existing `accepts_mouse_point`/`pick_mouse_entity` branches if it returns `false` — per this plan's Review Focus item on ordering.

- [ ] **Step 5: Gates and commit**

Run: `cargo build -p acad-app`, `cargo test -p acad-app`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`.

```bash
git add crates/acad-app/src/menu_panel.rs crates/acad-app/src/main.rs
git commit -m "feat(app): dispatch plain-text menu clicks through Editor::submit"
```

---

## Task 5: QEMU differential test for a real menu click

**Files:**
- Modify: `crates/acad-oracle/tests/commands.rs` (or a new `crates/acad-oracle/tests/menu_mouse.rs`, following whichever existing file this project's convention favors — check whether mouse-interaction tests are already split into their own file, e.g. `fillet_mouse.rs`, before deciding; if there is a `*_mouse.rs` convention, follow it and create `menu_mouse.rs`)

**Interfaces:**
- Consumes: `acad_oracle::session::Session`, `acad_oracle::mouse::device_for_pixel`, Task 1's recovered panel pixel geometry (for aiming the simulated click at a real entry, e.g. `LINE` or `ZOOM All`).
- Produces: one new `#[test]` proving a native mouse click on a chosen plain-text menu entry produces the same drawing-state change as typing that entry's macro — following `original_fillets_two_mouse_selected_lines`'s structure (`crates/acad-oracle/tests/fillet_mouse.rs:63-110`) for the native-side mouse simulation, and `original_dim_exports_primitive_geometry_matched_by_rust`'s structure (`crates/acad-oracle/tests/commands.rs:47-113`) for comparing native output against the Rust `Editor`'s own `submit` result.

- [ ] **Step 1: Write the test**

Pick a plain-text entry simple enough to verify by output shape alone — `ZOOM All` (`action = "zoom a"`) is a good choice: its effect (a view/limits change, not new geometry) is easy to assert on both sides without needing pixel-perfect entity comparison. Load `ACAD.MNU` natively (`MENU`, `ACAD`), then simulate a mouse click at that entry's recovered screen coordinates (`Session::mouse_pin`/`mouse_to`, per Task 1's findings), and separately drive the Rust `Editor` by calling `.submit("zoom a")` directly. Compare the resulting `drawing().header.view` (or whatever `ZOOM All`'s effect actually changes — confirm by reading the existing `ZOOM`-related command handling in `dispatch.rs` first) between the native DWG export and the Rust editor's own state.

- [ ] **Step 2: Run it**

Run: `cargo test -p acad-oracle --test <file> -- --nocapture` (substitute the actual test file name chosen in this task's Files section).
Expected: passes if Tasks 1-4 correctly reproduce native menu-click dispatch for a plain-text entry; if it fails, use `superpowers:systematic-debugging` — do not weaken the comparison to make it pass.

- [ ] **Step 3: Gates and commit**

Run the same four gates as every other task in this plan.

```bash
git add crates/acad-oracle/tests/<file>
git commit -m "test(oracle): add QEMU differential case for a native menu click"
```

---

## Task 6: Update the handover doc

**Files:**
- Modify: `docs/HANDOVER-2026-09-30.md` (or whatever the current dated handover file is named at execution time — check `docs/` for the most recent one first)

- [ ] **Step 1:** Update "Known limits and next work" item 3 to record what this plan completed (plain-text entry rendering and click dispatch, one differential case) and what remains explicitly out of scope per this plan's Global Constraints (control-byte entry dispatch semantics still unrecovered).
- [ ] **Step 2:** Commit.

```bash
git add docs/HANDOVER-*.md
git commit -m "docs: record screen-menu rendering and click dispatch in handover"
```
