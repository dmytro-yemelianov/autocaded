//! Reviewed finite native menu-control fixtures; mouse projection remains deferred.
use acad_cmd::{Editor, Effect, MenuControl};
use acad_model::Drawing;
use std::path::Path;

fn fixture(path: &str) -> Drawing {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/recovery/2026-10-01-menu-controls")
        .join(path);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("required fixture {}: {error}", path.display()));
    acad_dwg::parse(&bytes).unwrap()
}

fn baseline(on: bool) -> Editor {
    Editor::new(fixture(if on {
        "controls/drawings/BASEON.dwg"
    } else {
        "pilot/drawings/BASEOFF.dwg"
    }))
}

fn compare(actual: &Drawing, expected: &Drawing) {
    assert_eq!(actual.header.snap.on, expected.header.snap.on);
    assert_eq!(
        actual.header.snap.spacing.to_bits(),
        expected.header.snap.spacing.to_bits()
    );
    assert_eq!(actual.header.ortho, expected.header.ortho);
    // These typed-coordinate exports decode to the exact Rust doubles. Compare
    // complete items, including order, wrappers/layers, kind and all geometry.
    assert_eq!(actual.items, expected.items);
}

fn apply(editor: &mut Editor, control: MenuControl, status: &str) {
    assert_eq!(editor.apply_menu_control(control), Ok(Effect::Continue));
    assert_eq!(editor.status(), status);
}

#[test]
fn menu_control_snap_idle_matches_native() {
    let mut editor = baseline(false);
    apply(&mut editor, MenuControl::Snap, "<Snap on>");
    compare(editor.drawing(), &fixture("pilot/drawings/SCIDLE.dwg"));
}

#[test]
fn menu_control_ortho_idle_matches_native() {
    let mut editor = baseline(false);
    apply(&mut editor, MenuControl::Ortho, "<Ortho on>");
    compare(editor.drawing(), &fixture("pilot/drawings/OCIDLE.dwg"));
}

#[test]
fn menu_control_twice_and_initial_on_match_native() {
    for (control, on, off, twice, initial) in [
        (
            MenuControl::Snap,
            "<Snap on>",
            "<Snap off>",
            "SCTWICE",
            "SCON",
        ),
        (
            MenuControl::Ortho,
            "<Ortho on>",
            "<Ortho off>",
            "OCTWICE",
            "OCON",
        ),
    ] {
        let mut editor = baseline(false);
        apply(&mut editor, control, on);
        apply(&mut editor, control, off);
        compare(
            editor.drawing(),
            &fixture(&format!("controls/drawings/{twice}.dwg")),
        );
        let mut editor = baseline(true);
        apply(&mut editor, control, off);
        compare(
            editor.drawing(),
            &fixture(&format!("controls/drawings/{initial}.dwg")),
        );
    }
}

fn pending_line(control: MenuControl) -> Editor {
    let mut editor = baseline(false);
    for input in ["LINE", "2.1,3.15"] {
        editor.submit(input).unwrap();
    }
    editor.apply_menu_control(control).unwrap();
    assert_eq!(editor.prompt(), "LINE: next point (Enter to finish)");
    assert!(editor.drawing().items.is_empty());
    for input in ["4.25,5.15", ""] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.prompt(), "Command");
    editor
}

#[test]
fn menu_control_preserves_fractional_line_pending_state() {
    for (control, id) in [
        (MenuControl::Snap, "FSLINE"),
        (MenuControl::Ortho, "FOLINE"),
    ] {
        compare(
            pending_line(control).drawing(),
            &fixture(&format!("controls/drawings/{id}.dwg")),
        );
    }
}

#[test]
fn menu_control_preserves_circle_radius_pending_state() {
    for (control, id) in [
        (MenuControl::Snap, "SCIRCLE"),
        (MenuControl::Ortho, "OCIRCLE"),
    ] {
        let mut editor = baseline(false);
        for input in ["CIRCLE", "2,3"] {
            editor.submit(input).unwrap();
        }
        editor.apply_menu_control(control).unwrap();
        assert_eq!(editor.prompt(), "CIRCLE: radius");
        assert!(editor.drawing().items.is_empty());
        editor.submit("1.25").unwrap();
        assert_eq!(editor.prompt(), "Command");
        compare(
            editor.drawing(),
            &fixture(&format!("controls/drawings/{id}.dwg")),
        );
    }
}

#[test]
fn menu_control_cancel_retains_completed_segment() {
    let mut editor = Editor::default();
    for input in ["LINE", "2,3", "4,5"] {
        editor.submit(input).unwrap();
    }
    let completed = editor.drawing().items.clone();
    apply(&mut editor, MenuControl::Cancel, "*Cancel*");
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing().items, completed);
    for input in ["POINT", "8,7"] {
        editor.submit(input).unwrap();
    }
    compare(editor.drawing(), &fixture("pilot/drawings/CLDONE.dwg"));
}

#[test]
fn menu_control_cancel_discards_only_pending_geometry() {
    for (setup, id) in [
        (vec![], "CIDLE"),
        (vec!["LINE"], "CLSTART"),
        (vec!["LINE", "2,3"], "CLSEG"),
        (vec!["CIRCLE", "2,3"], "CCENTER"),
    ] {
        let mut editor = Editor::default();
        for input in setup {
            editor.submit(input).unwrap();
        }
        apply(&mut editor, MenuControl::Cancel, "*Cancel*");
        assert_eq!(editor.prompt(), "Command");
        assert!(editor.drawing().items.is_empty());
        for input in ["POINT", "8,7"] {
            editor.submit(input).unwrap();
        }
        compare(
            editor.drawing(),
            &fixture(&format!("cancel/drawings/{id}.dwg")),
        );
    }
}

#[test]
fn menu_control_cancel_retains_repeat_marker() {
    let mut editor = Editor::default();
    for input in ["REPEAT", "POINT", "4,5"] {
        editor.submit(input).unwrap();
    }
    let point = editor.drawing().items.clone();
    apply(&mut editor, MenuControl::Cancel, "*Cancel*");
    editor.submit("ENDREP").unwrap();
    assert_eq!(editor.prompt(), "ENDREP: columns");
    assert_eq!(editor.drawing().items, point);
    // CREPEAT's native DWG is intentionally not parsed: accepted evidence is
    // the Number of columns prompt and raw DXF, not a completed repeat group.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/recovery/2026-10-01-menu-controls/cancel/drawings");
    assert!(root.join("CREPEAT.dxf").is_file());
    assert!(root.join("CREPEAT.parse-error.txt").is_file());
}

#[test]
#[cfg(unix)]
fn original_menu_controls_pending_snap_matches_native() {
    use acad_oracle::{
        mouse::{device_for_pixel, LEFT},
        session::Session,
    };
    use std::{thread, time::Duration};
    const WAIT: Duration = Duration::from_secs(60);
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("SKIP native FSLINE replay: System.img or qemu-system-i386 unavailable");
        return;
    }
    // Exact accepted FSLINE collector setup; Session owns the serial guest
    // lock, disposable copy, bounded waits and the no-chain launch workaround.
    let mut vm = Session::boot_disposable(&disk, None, &[]).unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    vm.type_line("1").unwrap();
    vm.wait_for_text("Enter NAME of drawing:", WAIT).unwrap();
    vm.type_line("FSLINE").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", WAIT)
        .unwrap();
    thread::sleep(Duration::from_millis(500));
    for input in ["MENU", "ACAD"] {
        vm.type_line(input).unwrap();
        vm.capture_editor().unwrap();
    }
    vm.mouse_pin().unwrap();
    vm.capture_editor().unwrap();
    for input in [
        "SNAP", "0.5", "SNAP", "OFF", "ORTHO", "OFF", "LINE", "2.1,3.15",
    ] {
        vm.type_line(input).unwrap();
        vm.capture_editor().unwrap();
    }
    let (x, y) = device_for_pixel(600, 12);
    vm.mouse_to(x, y, 0).unwrap();
    vm.capture_editor().unwrap();
    vm.mouse_to(x, y, LEFT).unwrap();
    thread::sleep(Duration::from_millis(300));
    let pressed = vm.capture_editor();
    let release = vm.mouse_to(x, y, 0);
    release.unwrap();
    pressed.unwrap();
    thread::sleep(Duration::from_millis(300));
    vm.capture_editor().unwrap();
    for input in ["4.25,5.15", "", "END"] {
        vm.type_line(input).unwrap();
        vm.capture_editor().unwrap();
    }
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    // END already writes the DWG. No DXF export is needed for this regression.
    vm.shutdown().unwrap();
    let native = acad_dwg::parse(&vm.read_system_file("FSLINE.DWG").unwrap()).unwrap();
    let recorded = fixture("controls/drawings/FSLINE.dwg");
    compare(&native, &recorded);
    compare(pending_line(MenuControl::Snap).drawing(), &native);
    eprintln!("EXECUTED native FSLINE replay: controlled flags/spacing and full LINE match Rust and recorded fixture");
}
