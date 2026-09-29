#![cfg(unix)]

use acad_oracle::cga::{detect, Frame, Mode};
use acad_oracle::session::Session;
use std::ops::Range;
use std::path::Path;
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(30);

#[test]
fn monitor_follows_text_menu_graphics_editor_and_back() {
    let system = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !system.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let mut vm = Session::boot_disposable(&system, None, &[]).unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    assert_eq!(detect(&vm.video_memory().unwrap()), Some(Mode::Text));
    let menu = vm.screendump().unwrap();
    assert_eq!((menu.width, menu.height), (720, 400));
    // The BIOS copy of the CGA character ROM, used for text-mode frames.
    let font = vm.bios_font().unwrap();
    assert_eq!(
        font.glyph(b'A'),
        [0x30, 0x78, 0xCC, 0xCC, 0xFC, 0xCC, 0xCC, 0x00]
    );

    // Select "New drawing" through input-send-event, the runner's key path.
    for qcode in ["1", "ret"] {
        vm.key(qcode, true).unwrap();
        vm.key(qcode, false).unwrap();
        thread::sleep(Duration::from_millis(150));
    }
    vm.wait_for_text("Enter NAME of drawing:", WAIT).unwrap();
    vm.type_line("CGAMON").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", WAIT)
        .unwrap();
    thread::sleep(Duration::from_millis(500));
    for line in ["LINE", "1,1", "11,8", ""] {
        vm.type_line(line).unwrap();
    }
    let memory = vm.capture_editor().unwrap();
    assert_eq!(detect(&memory), Some(Mode::Graphics));
    let frame = Frame::new(&memory).unwrap();
    let lit = |xs: Range<usize>, ys: Range<usize>| {
        ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
            .filter(|&(x, y)| frame.lit(x, y))
            .count()
    };
    assert!(lit(572..640, 0..166) > 200, "screen menu missing");
    assert!(lit(0..640, 172..200) > 50, "prompt area missing");
    assert!(lit(0..560, 10..160) > 100, "drawn line missing");

    vm.type_line("END").unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    assert_eq!(detect(&vm.video_memory().unwrap()), Some(Mode::Text));
    vm.shutdown().unwrap();
    assert!(vm.exit_status().is_some());
}
