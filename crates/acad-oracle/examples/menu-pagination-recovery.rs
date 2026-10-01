//! One-shot recovery tool: drives native AutoCAD 1.4 under QEMU and captures
//! PNGs of the screen-menu panel's default page, then of what replaces it
//! after clicking the `NEXT` slot and separately after clicking the
//! `< GO >` slot, so a human/agent can read the panel's pagination
//! behavior and clickable-row geometry directly off the pixels.
//!
//! `Session::text_screen()` does not apply here for the same reason it does
//! not apply to the drawing editor's command-line prompts (see
//! `dim-angular-recovery.rs`'s module doc): AutoCAD draws its own screen
//! chrome, including the screen-menu panel, as bitmap graphics on the CGA
//! framebuffer, not as an 80x25 text page. Rendering the raw bitmap and
//! reading it directly avoids needing to reverse-engineer AutoCAD's own
//! on-screen font.
//!
//! Not a test — there is nothing to assert yet, only to observe. Run with:
//! `cargo run -p acad-oracle --example menu-pagination-recovery`

use acad_oracle::cga::Frame;
use acad_oracle::mouse::{device_for_pixel, LEFT};
use acad_oracle::session::Session;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);

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

fn save_capture(cga: &[u8], label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("menu-pagination-{label}.png"));
    match Frame::new(cga) {
        Ok(frame) => {
            save_png(&frame, &path);
            println!("  {label}: wrote {}", path.display());
        }
        Err(error) => println!("  {label}: frame decode failed: {error}"),
    }
    path
}

/// Move to, press, and release the left mouse button at pixel
/// `(column, row)`, settling on a captured frame after each phase exactly as
/// `fillet_mouse.rs:84-90` does. Returns the editor capture taken right
/// after release, which is what the click actually produced.
fn click_pixel(vm: &mut Session, column: usize, row: usize) -> Result<Vec<u8>, String> {
    let (x, y) = device_for_pixel(column, row);
    vm.mouse_to(x, y, 0)?;
    vm.capture_editor()?;
    vm.mouse_to(x, y, LEFT)?;
    thread::sleep(Duration::from_millis(300));
    vm.mouse_to(x, y, 0)?;
    thread::sleep(Duration::from_millis(300));
    vm.capture_editor()
}

fn main() {
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let mut vm = Session::boot_disposable(&disk, None, &[]).expect("boot");
    vm.wait_for_text("Enter selection:", WAIT)
        .expect("boot to main menu");
    vm.type_line("1").expect("choose create new drawing");
    vm.wait_for_text("Enter NAME of drawing:", WAIT)
        .expect("name prompt");
    vm.type_line("MENUPAGE").expect("drawing name");
    vm.wait_until_text_gone("Enter NAME of drawing:", WAIT)
        .expect("enter editor");
    thread::sleep(Duration::from_millis(500));

    println!("=== Step 1: default screen-menu page ===");
    let initial = vm.capture_editor().expect("capture initial screen");
    save_capture(&initial, "00-initial");

    // Row geometry measured directly from 00-initial.png (python/PIL scan of
    // lit-pixel bands in the panel's text columns, x in 572..640): 21 rows
    // of exactly 16 *display* pixels each (`< GO >` then 19 entries then
    // `NEXT`), starting at display-row 0: row r spans display rows
    // [16r, 16r+15]. `to_rgb_640x400()` doubles every native CGA scanline,
    // so `device_for_pixel`'s `row` argument (native CGA space, confirmed
    // by `fillet_mouse.rs`'s rows all being < 200) is HALF the display row
    // read off the PNG: native row = display_row / 2, i.e. row r's native
    // band is [8r, 8r+7], center 8r+4.
    let native_row_for_entry = |r: usize| 8 * r + 4;
    let go_row = native_row_for_entry(0); // row 0: `< GO >`
    let next_row = native_row_for_entry(20); // row 20: `NEXT` (19 entries: rows 1..=19)
    let column = 600usize; // inside the label text, clear of the x=568-571 divider

    vm.mouse_pin().expect("pin mouse");

    println!("=== Step 2a: click NEXT (native row {next_row}) ===");
    let next_click = click_pixel(&mut vm, column, next_row).expect("click NEXT");
    save_capture(&next_click, "01-after-next-click");

    // A second NEXT click (now on whatever page the first click produced)
    // tells us whether the file's 57 entries need more than one page turn,
    // and whether a further click past the last page wraps or no-ops.
    println!("=== Step 2b: click NEXT again ===");
    let next_click_2 = click_pixel(&mut vm, column, next_row).expect("click NEXT again");
    save_capture(&next_click_2, "02-after-second-next-click");

    println!("=== Step 2c: click NEXT a third time ===");
    let next_click_3 = click_pixel(&mut vm, column, next_row).expect("click NEXT a third time");
    save_capture(&next_click_3, "03-after-third-next-click");

    // `< GO >` is probed in the same session, right after the NEXT probing,
    // so this tool only ever boots one VM (fewer QEMU boots = fewer places
    // for the harness to hang).
    println!("=== Step 2d: click < GO > (native row {go_row}) ===");
    let go_click = click_pixel(&mut vm, column, go_row).expect("click < GO >");
    save_capture(&go_click, "04-after-go-click");
    vm.shutdown().ok();

    println!("done — read the saved PNGs by eye to record findings.");
}
