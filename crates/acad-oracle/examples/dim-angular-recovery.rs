//! One-shot recovery tool: drives native AutoCAD 1.4 under QEMU through
//! candidate `DIM` `A` (angular mode) input sequences, then captures and
//! renders the drawing editor's CGA framebuffer as a PNG so the actual
//! on-screen prompt text can be read by eye. `Session::text_screen()` does
//! not work here — it only decodes a genuine 80x25 alpha-text page, and
//! AutoCAD's drawing editor draws its own prompt text as bitmap graphics
//! (see `crates/acad-oracle/src/cga.rs`'s module doc), so a standard BIOS
//! font match cannot be assumed to apply. Rendering the raw bitmap and
//! reading it directly avoids needing to reverse-engineer AutoCAD's own
//! on-screen font.
//!
//! Not a test — there is nothing to assert yet, only to observe.
//! Run with: `cargo run -p acad-oracle --example dim-angular-recovery`

use acad_oracle::cga::Frame;
use acad_oracle::session::Session;
use std::path::Path;
use std::time::Duration;

fn save_png(frame: &Frame, path: &Path) {
    let pixels = frame.to_rgb_640x400();
    let mut pixmap = tiny_skia::Pixmap::new(640, 400).expect("640x400 pixmap");
    let data = pixmap.pixels_mut();
    for (index, &rgb) in pixels.iter().enumerate() {
        let r = ((rgb >> 16) & 0xFF) as u8;
        let g = ((rgb >> 8) & 0xFF) as u8;
        let b = (rgb & 0xFF) as u8;
        data[index] = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, 0xFF).unwrap();
    }
    pixmap.save_png(path).expect("write png");
}

/// Boot a fresh drawing, type `editor_lines`, then capture the stabilized
/// CGA framebuffer — independent of whether the drawing can later be
/// closed cleanly, unlike `acad_oracle::generate_pair`/`generate_visual_pair`,
/// which only return their capture on a fully successful run (including
/// `END` and the return to the drawing menu). A candidate whose guessed
/// input shape leaves AutoCAD mid-prompt is exactly the case this tool
/// needs to see, not one it can afford to fail on.
fn capture_stuck_frame(disk: &Path, name: &str, editor_lines: &[&str]) -> Result<Vec<u8>, String> {
    let mut vm = Session::boot_disposable(disk, None, &[])?;
    vm.wait_for_text("Enter selection:", Duration::from_secs(60))?;
    vm.type_line("1")?;
    vm.wait_for_text("Enter NAME of drawing:", Duration::from_secs(60))?;
    vm.type_line(name)?;
    vm.wait_until_text_gone("Enter NAME of drawing:", Duration::from_secs(60))?;
    std::thread::sleep(Duration::from_millis(500));
    for line in editor_lines {
        vm.type_line(line)?;
    }
    let cga = vm.capture_editor()?;
    vm.shutdown().ok();
    Ok(cga)
}

fn main() {
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping: extracted System.img or qemu-system-i386 absent");
        return;
    }

    // Same 6 candidates the blind generate_pair bisection already tried
    // (see the plan's recorded findings table), now captured visually
    // instead of only by whether the session could close cleanly.
    let candidates: [(&str, &[&str]); 6] = [
        (
            "DIMANG1",
            &[
                "LINE", "0,0", "5,0", "", "LINE", "0,0", "0,5", "", "DIM", "A", "0,0", "5,0",
                "0,0", "0,5", "3,3", "",
            ],
        ),
        (
            "DIMANG2",
            &[
                "LINE", "0,0", "5,0", "", "LINE", "0,0", "0,5", "", "DIM", "A", "1,0", "0,1",
                "3,3", "",
            ],
        ),
        (
            "DIMANG3",
            &[
                "LINE", "0,0", "5,0", "", "LINE", "0,0", "0,5", "", "DIM", "A", "",
            ],
        ),
        (
            "DIMANG4",
            &[
                "LINE", "0,0", "5,0", "", "LINE", "0,0", "0,5", "", "DIM", "A", "0,0", "",
            ],
        ),
        (
            "DIMANG5",
            &[
                "LINE", "0,0", "5,0", "", "LINE", "0,0", "0,5", "", "DIM", "A", "0,0", "5,0", "",
            ],
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
        match capture_stuck_frame(&disk, name, inputs) {
            Ok(cga) => match Frame::new(&cga) {
                Ok(frame) => {
                    let path = std::env::temp_dir().join(format!("dim-angular-{name}.png"));
                    save_png(&frame, &path);
                    println!("  wrote {}", path.display());
                }
                Err(error) => println!("  frame decode failed: {error}"),
            },
            Err(error) => println!("  capture failed: {error}"),
        }
        println!();
    }
}
