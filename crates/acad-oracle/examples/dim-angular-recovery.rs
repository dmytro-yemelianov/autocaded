//! One-shot recovery tool: drives native AutoCAD 1.4 under QEMU through
//! `DIM`'s `(ABCT)` sub-letters and captures a PNG of the drawing editor's
//! CGA framebuffer after every individual typed line, so the actual
//! on-screen prompt text can be read by eye. `Session::text_screen()` does
//! not work here — it only decodes a genuine 80x25 alpha-text page, and
//! AutoCAD's drawing editor draws its own prompt text as bitmap graphics
//! (see `crates/acad-oracle/src/cga.rs`'s module doc), so a standard BIOS
//! font match cannot be assumed to apply. Rendering the raw bitmap and
//! reading it directly avoids needing to reverse-engineer AutoCAD's own
//! on-screen font.
//!
//! The full `ABCT` picture is recovered this way (see
//! `docs/superpowers/plans/2026-09-30-dim-angular-recovery.md`): none are
//! an angular-dimensioning sub-mode. `A` sets dimension arrow size, `T`
//! toggles text orientation, and `B`/`C` (Baseline/Continue) only make
//! sense once a prior dimension exists to chain from. This run confirms
//! `B`/`C` after placing one first, and confirms that even a *valid*
//! arrow-size value returns straight to `Command:` rather than continuing
//! into the dimensioning flow.
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

/// Boot a fresh drawing, then type `editor_lines` one at a time, capturing
/// and saving a numbered PNG of the stabilized CGA framebuffer after
/// *each* line — not once at the end. The previous revision of this tool
/// captured only after the whole sequence, which left 5 of 6 candidates
/// showing an identical frozen screen; that is far more consistent with a
/// capture taken before AutoCAD finished reacting to an early line than
/// with 5 different guesses coincidentally producing byte-identical
/// screens. Capturing after each line makes any such race visible instead
/// of silently absorbed into one end-of-sequence snapshot, and this still
/// works independent of whether the session can later close cleanly —
/// unlike `acad_oracle::generate_pair`/`generate_visual_pair`, which only
/// return their capture on a fully successful run (including `END` and
/// the return to the drawing menu).
fn capture_each_step(
    disk: &Path,
    name: &str,
    editor_lines: &[&str],
) -> Result<Vec<Vec<u8>>, String> {
    let mut vm = Session::boot_disposable(disk, None, &[])?;
    vm.wait_for_text("Enter selection:", Duration::from_secs(60))?;
    vm.type_line("1")?;
    vm.wait_for_text("Enter NAME of drawing:", Duration::from_secs(60))?;
    vm.type_line(name)?;
    vm.wait_until_text_gone("Enter NAME of drawing:", Duration::from_secs(60))?;
    std::thread::sleep(Duration::from_millis(500));
    let mut frames = Vec::with_capacity(editor_lines.len());
    for line in editor_lines {
        vm.type_line(line)?;
        frames.push(vm.capture_editor()?);
    }
    vm.shutdown().ok();
    Ok(frames)
}

/// Run one candidate and save a numbered, labeled PNG per step, printing
/// where each one landed (or the error, if the session itself failed).
fn run_and_save(disk: &Path, name: &str, inputs: &[&str]) {
    println!("=== {name}: {inputs:?} ===");
    match capture_each_step(disk, name, inputs) {
        Ok(frames) => {
            for (step, cga) in frames.iter().enumerate() {
                match Frame::new(cga) {
                    Ok(frame) => {
                        let label = if inputs[step].is_empty() {
                            "blank".to_owned()
                        } else {
                            inputs[step].replace([',', ' '], "_")
                        };
                        let path = std::env::temp_dir()
                            .join(format!("dim-angular-{name}-{step:02}-{label}.png"));
                        save_png(&frame, &path);
                        println!(
                            "  step {step:02} ({:?}): wrote {}",
                            inputs[step],
                            path.display()
                        );
                    }
                    Err(error) => println!(
                        "  step {step:02} ({:?}): frame decode failed: {error}",
                        inputs[step]
                    ),
                }
            }
        }
        Err(error) => println!("  capture failed: {error}"),
    }
    println!();
}

fn main() {
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping: extracted System.img or qemu-system-i386 absent");
        return;
    }

    // B and C were rejected outright in a fresh drawing with no dimension
    // yet placed (consistent with Baseline/Continue needing one to chain
    // from). Retry both after placing one ordinary linear dimension first
    // (the same points as the existing DIMSHORT regression case), to see
    // whether they behave differently once that precondition is met.
    run_and_save(
        &disk,
        "DIMBASE",
        &["DIM", "1,1", "5,1", "3,2", "", "DIM", "B"],
    );
    run_and_save(
        &disk,
        "DIMCONT",
        &["DIM", "1,1", "5,1", "3,2", "", "DIM", "C"],
    );

    // "DIM", "A", "2", then the DIMSHORT points hung waiting to close under
    // generate_pair's all-or-nothing result — capture every step visually
    // instead, exactly as Revision 3 did for the original A-plus-more-input
    // hang.
    run_and_save(
        &disk,
        "DIMARBIG",
        &["DIM", "A", "2", "1,1", "5,1", "3,2", ""],
    );
}
