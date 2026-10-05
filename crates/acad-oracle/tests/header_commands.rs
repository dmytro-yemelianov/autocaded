//! Commands whose only lasting effect is the drawing header: the same typed
//! inputs run in the original ACAD.EXE (QEMU) and in the Rust editor, and
//! the saved header fields must agree.
#![cfg(unix)]

use acad_cmd::Editor;
use acad_model::Drawing;

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() && acad_oracle::available() {
        return Some(disk);
    }
    assert!(
        std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
        "System.img or qemu-system-i386 absent with AUTOCAD_REQUIRE_CORPUS set"
    );
    eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
    None
}

fn original(disk: &std::path::Path, name: &str, inputs: &[&str]) -> Drawing {
    let dwg = acad_oracle::generate_dwg(disk, name, inputs).unwrap();
    acad_dwg::parse(&dwg).unwrap()
}

fn native(inputs: &[&str]) -> Drawing {
    let mut editor = Editor::default();
    for input in inputs {
        editor
            .submit(input)
            .unwrap_or_else(|e| panic!("{input:?}: {e}"));
    }
    editor.drawing().clone()
}

#[test]
fn axis_settings_match_the_original_header() {
    let Some(disk) = disk() else { return };
    for (name, inputs) in [
        ("ORHAXON", &["AXIS", "ON"][..]),
        ("ORHAXOF", &["AXIS", "ON", "AXIS", "OFF"]),
        ("ORHAX5", &["AXIS", "5"]),
        ("ORHAX5X", &["SNAP", "0.5", "AXIS", "4X"]),
    ] {
        let original = original(&disk, name, inputs).header;
        let rust = native(inputs).header;
        assert_eq!(rust.axis, original.axis, "{name} {inputs:?}: AXIS");
        assert_eq!(rust.snap, original.snap, "{name} {inputs:?}: SNAP");
    }
}

#[test]
fn pan_moves_the_saved_view_like_the_original() {
    let Some(disk) = disk() else { return };
    for (name, inputs) in [
        ("ORHPAN1", &["PAN", "1,1", "4,3"][..]),
        ("ORHPAN2", &["ZOOM", "2", "PAN", "6,5", "2,7"]),
        ("ORHPAN3", &["PAN", "@3,-2", ""]),
    ] {
        let original = original(&disk, name, inputs).header.view;
        let rust = native(inputs).header.view;
        let close = |actual: f64, expected: f64, what: &str| {
            assert!(
                (actual - expected).abs() <= 1e-6,
                "{name} {inputs:?} {what}: Rust {actual} != original {expected}"
            );
        };
        close(rust.center.x, original.center.x, "view center x");
        close(rust.center.y, original.center.y, "view center y");
        close(rust.height, original.height, "view height");
    }
}
