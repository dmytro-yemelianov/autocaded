#![cfg(unix)]
//! Task 5 of the screen-menu-rendering plan: a native mouse click on a real
//! screen-menu entry must produce the same drawing-state change as typing
//! that entry's macro. `ZOOM All` (`action = "zoom a"`) is the chosen entry:
//! its effect (a view/limits change, not new geometry) is easy to assert on
//! both sides without pixel-perfect entity comparison.
//!
//! `ZOOM All` is not on `ACAD.MNU`'s first page: per Task 1's recovered
//! pagination (`docs/superpowers/plans/2026-10-01-screen-menu-rendering.md`,
//! Task 1 section), page 1 is `^Snap`..`^Cancel` (19 items plus `< GO >`),
//! and `ZOOM All` is the 8th item of page 2 (`POINT`..`^Cancel`, 18 items,
//! reached by one `NEXT` click). Page 2 has no `< GO >` header row, so Task
//! 1 found it leaves its two unused item slots blank *at the top* of the
//! 1..19 item range rather than next to `NEXT`: panel row 0 and row 1 are
//! blank, row 2 is `POINT`, row 3 `TRACE`, row 4 `SOLID`, row 5 `ARRAY`, row
//! 6 `HATCH`, row 7 `SKETCH`, row 8 `DIM`, and row 9 is `ZOOM All`.
//!
//! Row geometry (Task 1): row `r`'s *native* (undoubled CGA) pixel band is
//! `[8r, 8r+7]`, center `8r+4` — `device_for_pixel`'s `row` argument is
//! native space, not the doubled 640x400 display space screenshots are in.
//! `NEXT`'s fixed slot is row 20. Column 600 is inside every label's text,
//! clear of the panel's divider at native column ~568.
//!
//! These coordinates were verified before being committed here (see
//! `task-5-report.md` for what a scratch, since-deleted verification probe's
//! screenshots showed): a click at native row 76 (= 8*9+4, the `ZOOM All`
//! row computed above) after one `NEXT` click produced a Command-line read
//! of `Command: zoom Magnification or type (ACELPW): a` — i.e. the native
//! dispatcher had already processed `zoom` as one input and was about to
//! receive `a`, proving the click lands exactly on `ZOOM All` and not a
//! neighboring row (`DIM` above or `ZOOM Win` below would echo
//! differently).
//!
//! Rigor notes added by this test's own review round:
//! - `LIMITS` is set to a box that does *not* contain the origin
//!   (`(5,7)`-`(29,25)`) before the click, on both the native and Rust
//!   sides. `ZOOM All` fully recomputes `header.view` from `LIMITS`/
//!   `EXTENTS` (it does not depend on whatever the view was beforehand), so
//!   using a box whose correct fit-to-screen result is far from either
//!   side's own pre-click default view is what lets this test actually
//!   fail if the click missed, never fired, or hit a neighboring entry
//!   (which would leave the view at its unrelated pre-click default, not
//!   at the real `ZOOM All` answer for this box). A box not containing the
//!   origin was also necessary to catch a real formula bug this review
//!   round found: `ZOOM All`'s shown region is anchored relative to the
//!   origin — `union(LIMITS, EXTENTS)`, where a fresh drawing's `EXTENTS`
//!   defaults to a degenerate point *at the origin* on both sides — not
//!   anchored at `LIMITS`'s own lower-left corner, which is
//!   indistinguishable from the origin-anchored case for any box that
//!   already contains `(0, 0)`.
//! - The Rust side does not hand-type `"zoom a"` as a stand-in for the
//!   macro: it reads `ACAD.MNU`'s actual `ZOOM All` entry via
//!   `acad_cmd::menu::parse_menu` and splits its real `action` bytes the
//!   same way `acad-app`'s `split_macro_pieces` does (that function is
//!   private to the `acad-app` binary, so this test reimplements the one
//!   rule that matters for this macro: `;` and whitespace both act as
//!   Enter), asserting the split matches `["zoom", "a"]` before submitting
//!   those exact pieces — so this test is tied to the real decoded macro a
//!   click actually sends, not a hand-typed guess at it.
use acad_oracle::mouse::{device_for_pixel, LEFT};
use acad_oracle::session::Session;
use std::path::Path;
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);

/// Reimplements `acad-app`'s `split_macro_pieces` (private to that binary):
/// `;` and whitespace both act as a native macro's "Enter" between pieces.
fn split_macro_pieces(text: &str) -> Vec<&str> {
    text.split(';')
        .flat_map(|piece| {
            let mut words: Vec<&str> = piece.split_whitespace().collect();
            if words.is_empty() {
                words.push("");
            }
            words
        })
        .collect()
}

#[test]
fn original_zoom_all_menu_click_matches_the_typed_macro() {
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    // The real decoded macro, split exactly as a click would submit it —
    // not a hand-typed stand-in. `ACAD.MNU`'s `[ZOOM All]zoom a` must
    // decode to action bytes `b"zoom a"` (confirmed by `acad-cmd`'s own
    // `menu.rs` unit test) and split into two pieces.
    let menu = acad_cmd::menu::parse_menu(include_bytes!("../../../corpus/System/ACAD.MNU"))
        .expect("parse ACAD.MNU");
    let zoom_all = menu
        .entries
        .iter()
        .find(|entry| entry.label == "ZOOM All")
        .expect("ACAD.MNU has a ZOOM All entry");
    let action_text = std::str::from_utf8(&zoom_all.action).expect("action is valid UTF-8");
    let pieces = split_macro_pieces(action_text);
    assert_eq!(
        pieces,
        vec!["zoom", "a"],
        "ZOOM All's real macro must split into exactly these two submissions"
    );

    // Native: a fresh drawing, LIMITS moved away from the origin (so this
    // test can tell a correct click from a missed/no-op one — see the
    // module doc), ACAD.MNU loaded, NEXT clicked once, then ZOOM All's row.
    let mut vm = Session::boot_disposable(&disk, None, &[]).unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    vm.type_line("1").unwrap();
    vm.wait_for_text("Enter NAME of drawing:", WAIT).unwrap();
    vm.type_line("ZMCLICK").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", WAIT)
        .unwrap();
    thread::sleep(Duration::from_millis(500));

    vm.type_line("LIMITS").unwrap();
    vm.type_line("5,7").unwrap();
    vm.type_line("29,25").unwrap();

    vm.type_line("MENU").unwrap();
    vm.type_line("ACAD").unwrap();
    thread::sleep(Duration::from_millis(500));
    vm.capture_editor().unwrap();

    vm.mouse_pin().unwrap();
    let column = 600usize;
    let native_row_for_entry = |r: usize| 8 * r + 4;
    let next_row = native_row_for_entry(20);
    let zoom_all_row = native_row_for_entry(9);

    for row in [next_row, zoom_all_row] {
        let (x, y) = device_for_pixel(column, row);
        vm.mouse_to(x, y, 0).unwrap();
        vm.capture_editor().unwrap();
        vm.mouse_to(x, y, LEFT).unwrap();
        thread::sleep(Duration::from_millis(300));
        vm.mouse_to(x, y, 0).unwrap();
        thread::sleep(Duration::from_millis(300));
    }
    vm.capture_editor().unwrap();

    vm.type_line("END").unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    vm.shutdown().unwrap();

    let dwg = vm.read_system_file("ZMCLICK.DWG").unwrap();
    let native = acad_dwg::parse(&dwg).unwrap();
    let native_view = native.header.view;

    // Rust: the identical LIMITS, the real decoded macro pieces submitted
    // one at a time (matching how a real click dispatches them), instead
    // of a mouse click.
    let mut rust = acad_cmd::Editor::default();
    rust.submit("LIMITS").unwrap();
    rust.submit("5,7").unwrap();
    rust.submit("29,25").unwrap();
    for piece in &pieces {
        rust.submit(piece).unwrap();
    }
    let rust_view = rust.drawing().header.view;

    // This box's correct ZOOM All answer (union with the origin, since
    // EXTENTS defaults to a degenerate point there on both sides) is
    // `(0,0)-(29,25)`: height-constrained, height 25, center
    // `(25 * ZOOM_ALL_DEVICE_ASPECT / 2, 12.5)` — see
    // `original_zoom_all_fits_limits_matched_by_rust` in `commands.rs` and
    // `zoom_all_fits_the_union_of_limits_and_extents_anchored_at_the_origin`
    // in `acad-cmd`'s own `editor.rs` for the same box's committed,
    // independently-derived evidence. Neither side's pre-click default view
    // (Rust: center (0,0) height 20; native: center
    // (6.8504901960784315, 4.5) height 9.0) is anywhere near this, so a
    // missed click, a dead click, or a wrong formula that happened to leave
    // the view unchanged would all be caught by the assertions below.
    assert!(
        (native_view.height - 9.0).abs() > 1.0,
        "native view did not move away from its pre-click default — click had no effect: {native_view:?}"
    );
    assert!(
        (rust_view.center.x - 0.0).abs() > 1.0 || (rust_view.height - 20.0).abs() > 1.0,
        "rust view did not move away from its pre-click default: {rust_view:?}"
    );

    let close = |actual: f64, expected: f64, tolerance: f64, what: &str| {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{what}: {actual} != {expected}"
        );
    };
    close(
        native_view.center.x,
        rust_view.center.x,
        1e-6,
        "view center.x",
    );
    close(
        native_view.center.y,
        rust_view.center.y,
        1e-6,
        "view center.y",
    );
    close(native_view.height, rust_view.height, 1e-6, "view height");

    // Pin the actual numbers down too, not just "the two sides agree" (two
    // sides could agree on a shared wrong answer if both had the same
    // formula bug) — cross-check against the independently-committed
    // keyboard-only oracle evidence for this exact box.
    close(
        native_view.height,
        25.0,
        1e-6,
        "view height vs. known answer",
    );
    close(
        native_view.center.y,
        12.5,
        1e-6,
        "view center.y vs. known answer",
    );
}
