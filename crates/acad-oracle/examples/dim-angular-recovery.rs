//! One-shot recovery tool: drives native AutoCAD 1.4 under QEMU through
//! candidate `DIM` `A` (angular mode) input sequences and prints the
//! resulting DWG entities, so the actual input shape the native editor
//! expects can be inferred from what it produced — the same blind
//! sequence-in/decoded-entities-out technique this crate's own
//! `original_dim_exports_primitive_geometry_matched_by_rust` test already
//! uses for linear DIM (crates/acad-oracle/tests/commands.rs). Live
//! screen-probing does not work here: `Session::text_screen()` only
//! decodes a full-screen 80x25 text page, and AutoCAD's drawing editor is
//! a graphics/mixed-mode view it cannot read.
//!
//! Not a test — there is nothing to assert yet, only to observe and print.
//! Run with: `cargo run -p acad-oracle --example dim-angular-recovery`

use std::path::Path;

fn main() {
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping: extracted System.img or qemu-system-i386 absent");
        return;
    }

    // Each candidate is a full command-line input sequence, run blind from
    // a fresh drawing (2 reference lines are drawn first: 0,0-5,0 and
    // 0,0-0,5, giving a right-angle corner at the origin). Whatever extra
    // lines/points a candidate's angular flow doesn't actually consume get
    // fed back to the command prompt as literal text (harmless: AutoCAD's
    // command-line at worst reports "unknown command" and keeps going), so
    // a too-long guess is safe to try; a too-short one just leaves trailing
    // inputs unconsumed and visible in the final entity list as no-ops.
    let candidates: [(&str, &[&str]); 3] = [
        (
            "DIMANG4",
            &["LINE", "0,0", "5,0", "", "LINE", "0,0", "0,5", "", "DIM", "A", "0,0", ""],
        ),
        (
            "DIMANG5",
            &["LINE", "0,0", "5,0", "", "LINE", "0,0", "0,5", "", "DIM", "A", "0,0", "5,0", ""],
        ),
        (
            "DIMANG6",
            &[
                "LINE", "0,0", "5,0", "", "LINE", "0,0", "0,5", "", "DIM", "A", "0,0", "5,0",
                "0,0", "",
            ],
        ),
    ];

    for (name, inputs) in candidates {
        println!("=== candidate {name}: {inputs:?} ===");
        match acad_oracle::generate_pair(&disk, name, inputs) {
            Ok((dwg, _dxf)) => match acad_dwg::parse(&dwg) {
                Ok(drawing) => {
                    println!("  parsed OK, {} items:", drawing.items.len());
                    for (index, item) in drawing.items.iter().enumerate() {
                        println!("    [{index}] {item:?}");
                    }
                }
                Err(error) => println!("  DWG parse failed: {error}"),
            },
            Err(error) => println!("  generate_pair failed: {error}"),
        }
        println!();
    }
}
