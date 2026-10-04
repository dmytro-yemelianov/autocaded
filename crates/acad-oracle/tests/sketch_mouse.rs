#![cfg(unix)]
//! Original mouse SKETCH behaviour (docs/native-sketch.md rows O1–O14).
//!
//! The original ACAD.EXE runs under QEMU with its configured Mouse Systems
//! serial mouse. Each test moves the pointer, clicks the left button and
//! types control keys. It reads the graphics-mode command lines by matching
//! the BIOS 8×8 font and parses the saved DWG. Pointer samples are taken at
//! the guest's polling rate, so the tests assert only facts that do not depend
//! on which intermediate packet the guest sampled. Exact points (clicks, pen-up
//! tails, `.`, connect) get one CGA pixel of tolerance. Sampled vertices get
//! two. In the default view a CGA pixel is 1/40.8 world units wide and 1/17
//! world units tall, and drawing row `r` is world y `(161 - r) / 17`.
//! Where a rule is deterministic, the native editor replays the original's
//! vertices and must reach the same result.
use acad_model::{Entity, Item, Point};
use acad_oracle::mouse::{device_for_pixel, LEFT};
use acad_oracle::session::Session;
use std::path::Path;
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(60);
const PX: f64 = 1.0 / 40.8;
const ROW: f64 = 1.0 / 17.0;
const PROMPT: &str = "Sketch.  Pen eXit Quit Record Erase Connect .";

/// World point of CGA drawing pixel `(column, row)` in the default view.
fn w(column: f64, row: f64) -> Point {
    Point {
        x: column * PX,
        y: (161.0 - row) * ROW,
    }
}

fn near(a: Point, b: Point, pixels: f64) -> bool {
    (a.x - b.x).abs() <= pixels * PX + 1e-9 && (a.y - b.y).abs() <= pixels * ROW + 1e-9
}

struct Original {
    vm: Session,
    font: acad_oracle::cga::Font,
    name: &'static str,
}

impl Original {
    fn boot(name: &'static str) -> Option<Self> {
        let system = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
        if !system.exists() || !acad_oracle::available() {
            eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
            return None;
        }
        let mut vm = Session::boot_disposable(&system, None, &[]).unwrap();
        vm.wait_for_text("Enter selection:", WAIT).unwrap();
        vm.type_line("1").unwrap();
        vm.wait_for_text("Enter NAME of drawing:", WAIT).unwrap();
        vm.type_line(name).unwrap();
        vm.wait_until_text_gone("Enter NAME of drawing:", WAIT)
            .unwrap();
        thread::sleep(Duration::from_millis(500));
        let font = vm.bios_font().unwrap();
        vm.mouse_pin().unwrap();
        Some(Self { vm, font, name })
    }

    /// The three command lines at the bottom of the graphics screen.
    fn lines(&mut self) -> Vec<String> {
        let memory = self.vm.capture_editor().unwrap();
        let frame = acad_oracle::cga::Frame::new(&memory).unwrap();
        (22..25)
            .map(|row| {
                let text: String = (0..80)
                    .map(|column| {
                        let mut cell = [0u8; 8];
                        for (y, bits) in cell.iter_mut().enumerate() {
                            for x in 0..8 {
                                if frame.lit(column * 8 + x, row * 8 + y) {
                                    *bits |= 0x80 >> x;
                                }
                            }
                        }
                        (32u8..127)
                            .find(|&code| self.font.glyph(code) == cell)
                            .map_or('#', char::from)
                    })
                    .collect();
                text.trim().to_owned()
            })
            .collect()
    }

    fn last_line(&mut self) -> String {
        self.lines().pop().unwrap()
    }

    /// The guest drops serial packets while it is not reading; wait for a
    /// stable editor before every pointer or key event.
    fn settle(&mut self) {
        self.vm.capture_editor().unwrap();
    }

    fn type_line(&mut self, line: &str) {
        self.vm.type_line(line).unwrap();
    }

    fn key(&mut self, qcode: &str) {
        self.settle();
        self.vm.key(qcode, true).unwrap();
        thread::sleep(Duration::from_millis(80));
        self.vm.key(qcode, false).unwrap();
        thread::sleep(Duration::from_millis(300));
    }

    fn ctrl_c(&mut self) {
        self.settle();
        self.vm.key("ctrl", true).unwrap();
        self.vm.key("c", true).unwrap();
        thread::sleep(Duration::from_millis(80));
        self.vm.key("c", false).unwrap();
        self.vm.key("ctrl", false).unwrap();
        thread::sleep(Duration::from_millis(300));
    }

    fn move_to(&mut self, column: usize, row: usize) {
        self.settle();
        let (x, y) = device_for_pixel(column, row);
        self.vm.mouse_to(x, y, 0).unwrap();
        thread::sleep(Duration::from_millis(250));
    }

    fn click(&mut self) {
        self.settle();
        let (x, y) = self.vm.pointer();
        self.vm.mouse_to(x, y, LEFT).unwrap();
        thread::sleep(Duration::from_millis(300));
        self.vm.mouse_to(x, y, 0).unwrap();
        thread::sleep(Duration::from_millis(300));
    }

    fn click_at(&mut self, column: usize, row: usize) {
        self.move_to(column, row);
        self.click();
    }

    fn sketch(&mut self, increment: &str) {
        self.type_line("SKETCH");
        assert!(self
            .lines()
            .iter()
            .any(|line| line.starts_with("Record increment:")));
        self.type_line(increment);
        assert_eq!(self.last_line(), PROMPT, "O1");
    }

    fn finish(mut self) -> Vec<(Point, Point)> {
        self.settle();
        self.vm.type_line("END").unwrap();
        self.vm.wait_for_text("Enter selection:", WAIT).unwrap();
        self.vm.shutdown().unwrap();
        let dwg = self
            .vm
            .read_system_file(&format!("{}.DWG", self.name))
            .unwrap();
        acad_dwg::parse(&dwg)
            .unwrap()
            .items
            .iter()
            .map(|item| match item {
                Item::Entity(Entity::OnLayer { layer: 1, entity }) => match entity.as_ref() {
                    Entity::Line { start, end } => (*start, *end),
                    other => panic!("SKETCH wrote {other:?}"),
                },
                other => panic!("SKETCH wrote {other:?}"),
            })
            .collect()
    }
}

fn rust_sketch(increment: &str) -> acad_cmd::Editor {
    let mut editor = acad_cmd::Editor::default();
    editor.submit("SKETCH").unwrap();
    editor.submit(increment).unwrap();
    editor
}

fn rust_lines(editor: &acad_cmd::Editor) -> Vec<(Point, Point)> {
    editor
        .drawing()
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Entity(Entity::OnLayer { entity, .. }) => match entity.as_ref() {
                Entity::Line { start, end } => Some((*start, *end)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// O1–O4, O14: prompts, click pen toggle, one-axis increment, pen-up tail,
/// Return as X.
#[test]
fn original_sketch_click_pen_axis_increment_and_return() {
    let Some(mut original) = Original::boot("SKMET") else {
        return;
    };
    original.sketch("1");
    original.click_at(100, 100);
    // 0.83 by 0.88 from the pen-down point: Euclidean 1.21, under 1 on each axis.
    original.move_to(134, 85);
    original.move_to(170, 100);
    original.click();
    // A second stroke whose diagonal moves exceed 1 vertically.
    original.click_at(200, 100);
    original.move_to(220, 80);
    original.move_to(240, 100);
    original.click();
    original.type_line("");
    let lines = original.lines();
    assert_eq!(lines[2], "Command:", "O14");
    let lines_recorded = lines[1].clone();
    let lines = original.finish();
    assert_eq!(lines_recorded, format!("{} lines recorded.", lines.len()));
    // O3: the first stroke chains from the click to the pen-up point and
    // never turns at (134,85). The guest may sample mid-way through the
    // second move, so the chain can hold more than one LINE.
    let second = lines
        .iter()
        .position(|(a, _)| near(*a, w(200.0, 100.0), 1.0))
        .expect("second stroke");
    let first = &lines[..second];
    assert!(!first.is_empty(), "{lines:?}");
    let (start, end) = (first[0].0, first.last().unwrap().1);
    assert!(near(start, w(100.0, 100.0), 1.0), "{start:?}");
    assert!(near(end, w(170.0, 100.0), 1.0), "{end:?}");
    for pair in first.windows(2) {
        assert_eq!(pair[0].1, pair[1].0, "{lines:?}");
    }
    assert!(
        !first.iter().any(|(_, b)| near(*b, w(134.0, 85.0), 2.0)),
        "O3: Euclidean-only move made a vertex: {lines:?}"
    );
    // O3: the second stroke turns at the top of the 1.18-unit vertical move.
    assert!(lines.len() >= second + 2, "{lines:?}");
    assert!(
        near(lines[second].1, w(220.0, 80.0), 2.0),
        "{:?}",
        lines[second]
    );
    assert!(near(lines.last().unwrap().1, w(240.0, 100.0), 1.0));

    let mut rust = rust_sketch("1");
    rust.submit_mouse_point(start).unwrap();
    rust.pointer_moved(w(134.0, 85.0)).unwrap();
    rust.submit_mouse_point(end).unwrap();
    rust.submit("").unwrap();
    assert_eq!(rust_lines(&rust), vec![(start, end)]);
}

/// O4: pen-up records a final segment shorter than the increment. O13: with
/// ORTHO on, the pen-down tail recorded by a pen-up, X or R is an L: a leg
/// along the larger displacement from the last vertex, then the other leg to
/// the pointer itself. After R, sketching continues from the pointer.
#[test]
fn original_sketch_short_pen_up_tail_and_ortho_tail() {
    let Some(mut original) = Original::boot("SKTAIL") else {
        return;
    };
    original.sketch("1");
    original.click_at(100, 100);
    // 10 pixels = 0.245, a quarter of the increment: no sample is a vertex.
    original.move_to(110, 100);
    original.click();
    original.type_line("");
    for line in ["ORTHO", "ON"] {
        original.type_line(line);
    }
    original.sketch("1");
    // Every tail below stays under the increment on both axes from the last
    // vertex, so only the tail can reach the pointer.
    // A: pen-up, vertical displacement larger (0.12 across, 0.59 up).
    original.click_at(100, 150);
    original.move_to(150, 150);
    original.move_to(155, 140);
    original.click();
    // H: pen-up, horizontal displacement larger (0.25 across, 0.18 up).
    original.click_at(200, 150);
    original.move_to(250, 150);
    original.move_to(260, 147);
    original.click();
    // X: pen down, vertical larger, then X.
    original.click_at(300, 150);
    original.move_to(300, 125);
    original.move_to(305, 120);
    original.key("x");
    original.sketch("1");
    // Re-sync the mouse mirror (pen up, nothing is drawn) in case the guest
    // dropped packets while it was not reading between the two commands.
    original.settle();
    original.vm.mouse_pin().unwrap();
    // R: pen down, vertical larger, R, then continue upward.
    original.click_at(400, 150);
    original.move_to(450, 150);
    original.move_to(455, 140);
    original.key("r");
    original.move_to(457, 120);
    original.click();
    original.key("x");
    let lines = original.finish();
    let (short, ortho): (Vec<&(Point, Point)>, Vec<_>) = lines.iter().partition(|(a, _)| a.y > 3.0);
    assert_eq!(short.len(), 1, "O4: {lines:?}");
    assert!(near(short[0].0, w(100.0, 100.0), 1.0), "{lines:?}");
    assert!(near(short[0].1, w(110.0, 100.0), 1.0), "O4: {lines:?}");
    for (a, b) in &ortho {
        assert!(a.x == b.x || a.y == b.y, "O13: {lines:?}");
    }
    let stroke = |from: f64, to: f64| -> Vec<(Point, Point)> {
        ortho
            .iter()
            .filter(|(a, b)| [a, b].iter().all(|p| p.x >= from * PX && p.x <= to * PX))
            .map(|line| **line)
            .collect()
    };
    let horizontal = |l: &(Point, Point)| l.0.y == l.1.y && l.0.x != l.1.x;
    let vertical = |l: &(Point, Point)| l.0.x == l.1.x && l.0.y != l.1.y;
    // Native replay: ORTHO, pen down at `start`, one vertex, pointer, finish.
    // The pointer is the original's own recorded end point, so both sides use
    // bit-identical coordinates.
    let replay = |start: Point, vertex: Point, pointer: Point, finish: &str| {
        let mut rust = rust_sketch("1");
        rust.drawing_mut().header.ortho = true;
        rust.submit_mouse_point(start).unwrap();
        rust.pointer_moved(vertex).unwrap();
        rust.pointer_moved(pointer).unwrap();
        match finish {
            "click" => {
                rust.submit_mouse_point(pointer).unwrap();
                rust.submit("X").unwrap();
            }
            "R" => {
                rust.submit("R").unwrap();
                rust.submit("Q").unwrap();
            }
            other => {
                rust.submit(other).unwrap();
            }
        }
        rust_lines(&rust)
    };
    let same = |a: &[(Point, Point)], b: &[(Point, Point)]| {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(x, y)| near(x.0, y.0, 0.01) && near(x.1, y.1, 0.01))
    };

    // A: vertical leg (non-degenerate) then horizontal leg to the pointer.
    let a = stroke(95.0, 160.0);
    let (leg1, leg2) = (a[a.len() - 2], a[a.len() - 1]);
    assert!(
        vertical(&leg1) && leg1.1.y - leg1.0.y >= 9.0 * ROW,
        "A: {a:?}"
    );
    assert!(horizontal(&leg2) && leg2.0 == leg1.1, "A: {a:?}");
    assert!(near(leg2.1, w(155.0, 140.0), 1.0), "A: {a:?}");
    let rust = replay(a[0].0, leg1.0, leg2.1, "click");
    assert!(
        same(&rust[rust.len() - 2..], &a[a.len() - 2..]),
        "A replay {rust:?} vs {a:?}"
    );

    // H: horizontal leg first, merged into the horizontal run, then vertical.
    let h = stroke(195.0, 265.0);
    let (leg1, leg2) = (h[h.len() - 2], h[h.len() - 1]);
    assert!(
        horizontal(&leg1) && near(leg1.1, w(260.0, 150.0), 1.0),
        "H: {h:?}"
    );
    assert!(vertical(&leg2) && leg2.0 == leg1.1, "H: {h:?}");
    assert!(near(leg2.1, w(260.0, 147.0), 1.0), "H: {h:?}");
    let rust = replay(h[0].0, leg1.1, leg2.1, "click");
    assert!(
        same(&rust[rust.len() - 2..], &h[h.len() - 2..]),
        "H replay {rust:?} vs {h:?}"
    );

    // X: the same L as a pen-up; the final leg is not lost.
    let x = stroke(295.0, 310.0);
    let (leg1, leg2) = (x[x.len() - 2], x[x.len() - 1]);
    assert!(
        vertical(&leg1) && near(leg1.1, w(300.0, 120.0), 1.0),
        "X: {x:?}"
    );
    assert!(horizontal(&leg2) && leg2.0 == leg1.1, "X: {x:?}");
    assert!(near(leg2.1, w(305.0, 120.0), 1.0), "X: {x:?}");
    let rust = replay(x[0].0, leg1.1, leg2.1, "X");
    assert!(
        same(&rust[rust.len() - 2..], &x[x.len() - 2..]),
        "X replay {rust:?} vs {x:?}"
    );

    // R: the L, then the next LINE starts at the pointer, not the corner.
    let r = stroke(395.0, 460.0);
    let k = r
        .iter()
        .position(|l| near(l.1, w(455.0, 140.0), 1.0))
        .unwrap_or_else(|| panic!("R tail to the pointer: {r:?} in {lines:?}"));
    assert!(k >= 1 && k + 1 < r.len(), "R: {r:?}");
    assert!(
        vertical(&r[k - 1]) && r[k - 1].1.y - r[k - 1].0.y >= 9.0 * ROW,
        "R: {r:?}"
    );
    assert!(horizontal(&r[k]) && r[k].0 == r[k - 1].1, "R: {r:?}");
    assert_eq!(r[k + 1].0, r[k].1, "R continues from the pointer: {r:?}");
    assert!(vertical(&r[k + 1]), "R: {r:?}");
    let rust = replay(r[0].0, r[k - 1].0, r[k].1, "R");
    assert!(
        same(&rust[rust.len() - 2..], &r[k - 1..=k]),
        "R replay {rust:?} vs {r:?}"
    );
}

/// O5, O6: collinear runs merge, reversals stay separate, and R records the
/// pen-down tail and continues from it.
#[test]
fn original_sketch_merges_runs_and_record_keeps_the_tail() {
    let Some(mut original) = Original::boot("SKRUN") else {
        return;
    };
    original.sketch(".25");
    original.click_at(100, 100);
    for column in [120, 140, 160] {
        original.move_to(column, 100);
    }
    original.move_to(160, 80);
    original.move_to(160, 110);
    original.move_to(161, 110);
    original.key("r");
    let recorded = original.last_line();
    original.move_to(190, 110);
    original.key("x");
    let lines = original.lines();
    assert_eq!(lines[2], "Command:");
    let exited = lines[1].clone();
    let lines = original.finish();
    // One horizontal LINE covers the whole run along row 100.
    let horizontal: Vec<_> = lines
        .iter()
        .filter(|(a, b)| a.y == b.y && near(*a, w(100.0, 100.0), 0.5))
        .collect();
    assert_eq!(horizontal.len(), 1, "{lines:?}");
    assert!(horizontal[0].1.x >= w(158.0, 100.0).x, "{lines:?}");
    // Up then down near column 160: two LINEs in opposite directions.
    let vertical = |a: &Point, b: &Point| (a.x - b.x).abs() <= 2.0 * PX;
    let up = lines
        .iter()
        .position(|(a, b)| vertical(a, b) && b.y > a.y + 10.0 * ROW)
        .expect("upward LINE");
    let down = lines
        .iter()
        .position(|(a, b)| vertical(a, b) && b.y < a.y - 10.0 * ROW)
        .expect("downward LINE");
    assert!(up < down, "{lines:?}");
    // R recorded through the exact pointer point (161,110), and X recorded
    // one more LINE starting there.
    let tail = w(161.0, 110.0);
    let split = lines
        .iter()
        .position(|(_, b)| near(*b, tail, 0.5))
        .expect("R tail at the pointer");
    assert_eq!(recorded, format!("{} lines recorded.", split + 1), "O6");
    assert_eq!(
        exited,
        format!("{} lines recorded.", lines.len() - split - 1)
    );
    assert_eq!(lines[split + 1].0, lines[split].1);
    assert!(near(lines.last().unwrap().1, w(190.0, 110.0), 1.0));
}

/// O7, O8: Q and Ctrl-C keep recorded LINEs and discard temporary ones.
#[test]
fn original_sketch_quit_and_cancel_discard_only_temporary_lines() {
    let Some(mut original) = Original::boot("SKQUIT") else {
        return;
    };
    original.sketch(".25");
    original.click_at(100, 100);
    original.move_to(150, 100);
    original.move_to(200, 120);
    original.key("r");
    original.move_to(250, 120);
    original.move_to(300, 60);
    original.key("q");
    assert_eq!(original.last_line(), "Command:", "O7");
    original.sketch(".25");
    original.click_at(100, 150);
    original.move_to(150, 150);
    original.click();
    original.key("r");
    original.click_at(300, 150);
    original.move_to(350, 140);
    original.move_to(400, 130);
    original.ctrl_c();
    assert_eq!(original.last_line(), "Command:", "O8");
    let lines = original.finish();
    assert!(!lines.is_empty());
    for (a, b) in &lines {
        for point in [a, b] {
            assert!(point.x <= w(201.0, 0.0).x, "temporary LINE kept: {lines:?}");
        }
    }
    assert!(lines.iter().any(|(a, _)| near(*a, w(100.0, 100.0), 1.0)));
    assert!(lines
        .iter()
        .any(|(a, b)| near(*a, w(100.0, 150.0), 1.0) && near(*b, w(150.0, 150.0), 1.0)));
}

/// O9–O11: `.` with pen up and down, connect messages, tolerance and the
/// exact resumed end point.
#[test]
fn original_sketch_dot_and_connect() {
    let Some(mut original) = Original::boot("SKCONN") else {
        return;
    };
    original.sketch(".25");
    original.move_to(100, 100);
    original.key("c");
    let lines = original.lines();
    assert_eq!(lines[0], "No last point known.", "O11");
    assert_eq!(lines[1], "Connect aborted.", "O11");
    assert_eq!(lines[2], PROMPT);
    original.click();
    original.move_to(150, 100);
    original.key("c");
    let lines = original.lines();
    assert_eq!(
        lines[0], "Connect command meaningless when pen down.",
        "O11"
    );
    assert_eq!(lines[1], "Connect aborted.");
    // `.` with the pen down changes nothing: the stroke continues.
    original.key("dot");
    original.move_to(200, 100);
    // O2: the P key raises the pen just as a click does.
    original.key("p");
    original.move_to(250, 50);
    original.key("dot");
    original.move_to(300, 50);
    original.key("c");
    assert_eq!(original.last_line(), "Connect:  Move to endpoint of line.");
    // 12 pixels = 0.294 > 0.25 from the end point: still connecting.
    original.move_to(262, 50);
    assert_eq!(original.last_line(), "Connect:  Move to endpoint of line.");
    // 8 pixels = 0.196: the pen goes down at the exact end point.
    original.move_to(258, 50);
    assert_eq!(original.last_line(), PROMPT, "O10");
    original.move_to(270, 80);
    original.click();
    // At the end point already: C connects at once.
    original.key("c");
    assert_eq!(original.last_line(), PROMPT, "O10");
    original.move_to(290, 80);
    original.click();
    original.key("x");
    let lines = original.finish();
    let has = |a: Point, b: Point, tolerance: f64| {
        lines
            .iter()
            .any(|(s, e)| near(*s, a, tolerance) && near(*e, b, tolerance))
    };
    assert!(
        has(w(100.0, 100.0), w(200.0, 100.0), 1.0),
        "O9 pen down: {lines:?}"
    );
    assert!(
        has(w(200.0, 100.0), w(250.0, 50.0), 1.0),
        "O9 pen up: {lines:?}"
    );
    // Connect resumes at the `.` line's exact end point, not the pointer.
    let dot = lines
        .iter()
        .find(|(s, e)| near(*s, w(200.0, 100.0), 1.0) && near(*e, w(250.0, 50.0), 1.0))
        .unwrap();
    assert!(lines.iter().any(|(s, _)| s == &dot.1), "O10: {lines:?}");
    assert!(!lines.iter().any(|(s, _)| near(*s, w(258.0, 50.0), 1.0)));
    assert!(
        has(w(270.0, 80.0), w(290.0, 80.0), 1.0),
        "O10 immediate: {lines:?}"
    );

    // The native editor applies the same tolerance to the same points.
    let mut rust = rust_sketch(".25");
    rust.submit_mouse_point(w(200.0, 100.0)).unwrap();
    rust.submit_mouse_point(w(200.0, 100.0)).unwrap();
    rust.pointer_moved(w(250.0, 50.0)).unwrap();
    rust.submit(".").unwrap();
    rust.pointer_moved(w(300.0, 50.0)).unwrap();
    rust.submit("C").unwrap();
    rust.pointer_moved(w(262.0, 50.0)).unwrap();
    assert!(!rust.sketch_preview().unwrap().pen_down);
    rust.pointer_moved(w(258.0, 50.0)).unwrap();
    assert!(rust.sketch_preview().unwrap().pen_down);
}

fn erase_base(original: &mut Original) {
    original.sketch(".25");
    original.click_at(100, 100);
    original.move_to(150, 100);
    original.move_to(200, 110);
    original.move_to(250, 100);
    original.click();
}

/// Replays an original temporary path into the native editor and erases
/// from `pointer`.
fn rust_erase(path: &[Point], pointer: Point) -> Vec<(Point, Point)> {
    let mut rust = rust_sketch(".25");
    rust.submit_mouse_point(path[0]).unwrap();
    for point in &path[1..] {
        rust.pointer_moved(*point).unwrap();
    }
    rust.submit_mouse_point(*path.last().unwrap()).unwrap();
    rust.submit("E").unwrap();
    rust.pointer_moved(pointer).unwrap();
    rust.submit("P").unwrap();
    rust.submit("X").unwrap();
    rust_lines(&rust)
}

/// O12: P cuts the path back through the vertex nearest the pointer.
#[test]
fn original_sketch_erase_cuts_back_from_the_path_end() {
    let Some(mut original) = Original::boot("SKERA1") else {
        return;
    };
    erase_base(&mut original);
    original.key("e");
    assert_eq!(original.last_line(), "Erase:  Select end of delete.");
    original.move_to(240, 100);
    original.key("p");
    assert_eq!(original.last_line(), PROMPT);
    original.key("x");
    let lines = original.finish();
    // The third line (199,110)→(249,100) and the pen-up stub are gone.
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(near(lines[1].1, w(200.0, 110.0), 2.0), "{lines:?}");
    let path = [
        lines[0].0,
        lines[0].1,
        lines[1].1,
        w(249.0, 100.0),
        w(250.0, 100.0),
    ];
    assert_eq!(rust_erase(&path, w(240.0, 100.0)), lines);
}

/// O12: a pointer next to the stroke's start erases every temporary line.
#[test]
fn original_sketch_erase_near_start_erases_everything() {
    let Some(mut original) = Original::boot("SKERA3") else {
        return;
    };
    erase_base(&mut original);
    original.key("e");
    original.move_to(101, 100);
    original.key("p");
    original.key("x");
    assert_eq!(original.lines()[1], "0 lines recorded.");
    let lines = original.finish();
    assert!(lines.is_empty(), "{lines:?}");
    let path = [
        w(100.0, 100.0),
        w(149.0, 100.0),
        w(199.0, 110.0),
        w(249.0, 100.0),
        w(250.0, 100.0),
    ];
    assert!(rust_erase(&path, w(101.0, 100.0)).is_empty());
}

/// O12: one pixel from the shared vertex (199,110), and nearer the interior
/// of the following line, P still erases the line ending there: the rule is
/// nearest vertex, not nearest line. E aborts.
#[test]
fn original_sketch_erase_uses_nearest_vertex_and_aborts() {
    let Some(mut original) = Original::boot("SKERA2") else {
        return;
    };
    erase_base(&mut original);
    original.key("e");
    original.move_to(200, 110);
    original.key("e");
    let lines = original.lines();
    assert_eq!(lines[1], "Erase aborted.");
    assert_eq!(lines[2], PROMPT);
    original.key("e");
    original.move_to(200, 110);
    original.key("p");
    original.key("x");
    let lines = original.finish();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(near(lines[0].0, w(100.0, 100.0), 1.0));
    let path = [
        lines[0].0,
        lines[0].1,
        w(199.0, 110.0),
        w(249.0, 100.0),
        w(250.0, 100.0),
    ];
    assert_eq!(rust_erase(&path, w(200.0, 110.0)), lines);
}

/// O13: SNAP applies to sketched points; ORTHO keeps vertices axis-aligned
/// from the previous vertex.
#[test]
fn original_sketch_snap_and_ortho() {
    let Some(mut original) = Original::boot("SKSNAP") else {
        return;
    };
    for line in ["SNAP", ".5", "SNAP", "ON"] {
        original.type_line(line);
    }
    original.sketch(".25");
    original.click_at(103, 102);
    original.move_to(151, 101);
    original.move_to(203, 97);
    original.move_to(252, 70);
    original.click();
    original.key("x");
    for line in ["SNAP", "OFF", "ORTHO", "ON"] {
        original.type_line(line);
    }
    original.sketch(".25");
    original.click_at(100, 150);
    original.move_to(150, 160);
    original.move_to(170, 130);
    original.move_to(200, 140);
    original.click();
    original.key("x");
    let lines = original.finish();
    let (snapped, ortho): (Vec<_>, Vec<_>) = lines.iter().partition(|(a, _)| a.y > 2.0);
    assert!(snapped.len() >= 2 && ortho.len() >= 2, "{lines:?}");
    let on_grid = |p: Point| {
        let grid = Point {
            x: (p.x * 2.0).round() / 2.0,
            y: (p.y * 2.0).round() / 2.0,
        };
        near(p, grid, 1.0)
    };
    for (a, b) in &snapped {
        assert!(on_grid(*a) && on_grid(*b), "O13 SNAP: {lines:?}");
    }
    for (a, b) in &ortho {
        assert!(a.x == b.x || a.y == b.y, "O13 ORTHO: {lines:?}");
    }
}
