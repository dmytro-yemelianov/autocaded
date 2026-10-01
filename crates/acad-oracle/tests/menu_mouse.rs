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
//! These coordinates were verified before being committed here: a scratch
//! probe (not part of this diff) clicked native row 76 (= 8*9+4, the `ZOOM
//! All` row computed above) after one `NEXT` click and captured the
//! Command-line text mid-press, which read
//! `Command: zoom Magnification or type (ACELPW): a` — i.e. the native
//! dispatcher had already processed `zoom` as one input and was about to
//! receive `a`, proving the click lands exactly on `ZOOM All` and not a
//! neighboring row (`DIM` above or `ZOOM Win` below would echo differently).
//! See `task-5-report.md` for the saved screenshot this read came from.
use acad_oracle::mouse::{device_for_pixel, LEFT};
use acad_oracle::session::Session;
use std::path::Path;
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);

#[test]
fn original_zoom_all_menu_click_matches_the_typed_macro() {
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    // Baseline: an untouched fresh drawing's own header, used to seed the
    // Rust `Editor` below. `acad_cmd::Editor::default()`'s fallback LIMITS
    // (-10..10 both axes) do not match the native fresh-drawing default
    // (0..12 x 0..9, confirmed by this same baseline read) — an unrelated,
    // already-accepted simplification elsewhere in this crate, not
    // something this test should also be re-proving. Seeding both sides
    // from the same real header isolates `ZOOM All`'s own effect, matching
    // `original_resolution_aliases_update_the_shared_snap_header_state` and
    // `redraw_and_regen_are_display_only_commands_in_the_original`'s own
    // precedent of seeding the Rust editor from the native header.
    let baseline_dwg = acad_oracle::generate_dwg(&disk, "ZMBASE", &[]).unwrap();
    let baseline_header = acad_dwg::parse(&baseline_dwg).unwrap().header;

    // Native: load ACAD.MNU, click NEXT once, then click ZOOM All's row.
    let mut vm = Session::boot_disposable(&disk, None, &[]).unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    vm.type_line("1").unwrap();
    vm.wait_for_text("Enter NAME of drawing:", WAIT).unwrap();
    vm.type_line("ZMCLICK").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", WAIT)
        .unwrap();
    thread::sleep(Duration::from_millis(500));

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

    // Rust: identical starting header, typing the macro instead of clicking
    // it. `Editor::submit` only ever matches a whole trimmed line against
    // one keyword (`crates/acad-cmd/src/dispatch.rs`'s `command`/`Zoom`
    // state match), so `"zoom a"` is submitted as the two separate pieces a
    // real click actually produces (see `acad-app`'s `split_macro_pieces`,
    // which this test's own recovery under QEMU required fixing to also
    // split on whitespace, not only `;`).
    let mut rust = acad_cmd::Editor::new(acad_model::Drawing {
        header: baseline_header,
        items: Vec::new(),
    });
    rust.submit("zoom").unwrap();
    rust.submit("a").unwrap();

    let native_view = native.header.view;
    let rust_view = rust.drawing().header.view;
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
}
