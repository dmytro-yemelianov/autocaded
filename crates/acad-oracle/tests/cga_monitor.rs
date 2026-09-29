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

/// The crosshair's column and row in a CGA frame, ignoring the fixed screen
/// dividers at rows 168–170 and columns 568–571.
fn crosshair(memory: &[u8]) -> (Option<usize>, Option<usize>) {
    let frame = Frame::new(memory).unwrap();
    let row = (0..200)
        .filter(|y| !(168..=170).contains(y))
        .find(|&y| (0..560).filter(|&x| frame.lit(x, y)).count() > 400);
    let column = (0..640)
        .filter(|x| !(568..=571).contains(x))
        .find(|&x| (10..165).filter(|&y| frame.lit(x, y)).count() > 120);
    (column, row)
}

#[test]
fn serial_mouse_points_at_the_requested_pixel_and_picks() {
    use acad_oracle::mouse::{device_for_pixel, LEFT};
    let system = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !system.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let mut vm = Session::boot_disposable(&system, None, &[]).unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    vm.type_line("1").unwrap();
    vm.wait_for_text("Enter NAME of drawing:", WAIT).unwrap();
    vm.type_line("MOUSEPT").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", WAIT)
        .unwrap();
    thread::sleep(Duration::from_millis(500));
    // The crosshair is drawn while AutoCAD waits for a point.
    vm.type_line("LINE").unwrap();
    vm.mouse_pin().unwrap();
    for (column, row) in [(156, 134), (312, 87), (468, 41), (40, 150)] {
        let (x, y) = device_for_pixel(column, row);
        vm.mouse_to(x, y, 0).unwrap();
        assert_eq!(vm.pointer(), (x, y));
        let (seen_column, seen_row) = crosshair(&vm.capture_editor().unwrap());
        assert_eq!(seen_column, Some(column), "column for {column},{row}");
        let seen_row = seen_row.expect("crosshair row");
        assert!(
            seen_row.abs_diff(row) <= 1,
            "row {seen_row} for {column},{row}"
        );
    }

    // Pick two points with the left button; the line joins them.
    for (column, row) in [(100, 150), (400, 50)] {
        let (x, y) = device_for_pixel(column, row);
        vm.mouse_to(x, y, 0).unwrap();
        vm.mouse_to(x, y, LEFT).unwrap();
        thread::sleep(Duration::from_millis(300));
        vm.mouse_to(x, y, 0).unwrap();
        thread::sleep(Duration::from_millis(300));
    }
    vm.type_line("").unwrap();
    // Park the pointer on the screen menu so the crosshair is not drawn.
    let (x, y) = device_for_pixel(600, 100);
    vm.mouse_to(x, y, 0).unwrap();
    let memory = vm.capture_editor().unwrap();
    let frame = Frame::new(&memory).unwrap();
    let near = |cx: usize, cy: usize| {
        (cy - 2..=cy + 2).any(|y| (cx - 2..=cx + 2).any(|x| frame.lit(x, y)))
    };
    assert!(near(250, 100), "picked line missing at its midpoint");
    assert!(
        near(175, 125) && near(325, 75),
        "picked line missing at its quarters"
    );
    vm.shutdown().unwrap();
}
