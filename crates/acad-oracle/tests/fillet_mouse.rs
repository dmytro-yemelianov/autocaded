#![cfg(unix)]

use acad_oracle::mouse::{device_for_pixel, LEFT};
use acad_oracle::session::Session;
use std::path::Path;
use std::thread;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(30);

fn assert_geometry_matches(original: &[acad_model::Item], editor: &[acad_model::Item]) {
    use acad_model::{Entity, Item};

    assert_eq!(editor.len(), original.len());
    let close = |actual: f64, expected: f64| (actual - expected).abs() < 1e-9;
    let same_point = |actual: acad_model::Point, expected: acad_model::Point| {
        close(actual.x, expected.x) && close(actual.y, expected.y)
    };
    for (ours, original) in editor.iter().zip(original) {
        let (
            Item::Entity(Entity::OnLayer {
                layer: ours_layer,
                entity: ours,
            }),
            Item::Entity(Entity::OnLayer {
                layer: original_layer,
                entity: original,
            }),
        ) = (ours, original)
        else {
            panic!("FILLET entity wrappers differ: {ours:?} vs {original:?}");
        };
        assert_eq!(ours_layer, original_layer);
        match (ours.as_ref(), original.as_ref()) {
            (Entity::Line { start: a, end: b }, Entity::Line { start: c, end: d }) => {
                assert!(
                    same_point(*a, *c) && same_point(*b, *d),
                    "LINE differs: {a:?} {b:?} vs {c:?} {d:?}"
                );
            }
            (
                Entity::Arc {
                    center: a,
                    radius: ar,
                    start_deg: as_,
                    end_deg: ae,
                },
                Entity::Arc {
                    center: b,
                    radius: br,
                    start_deg: bs,
                    end_deg: be,
                },
            ) => assert!(
                same_point(*a, *b) && close(*ar, *br) && close(*as_, *bs) && close(*ae, *be),
                "ARC differs: {ours:?} vs {original:?}"
            ),
            _ => panic!("FILLET entity types differ: {ours:?} vs {original:?}"),
        }
    }
}

#[test]
fn original_fillets_two_mouse_selected_lines() {
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
    vm.type_line("FILLET1").unwrap();
    vm.wait_until_text_gone("Enter NAME of drawing:", WAIT)
        .unwrap();
    thread::sleep(Duration::from_millis(500));

    // The pointer moves in device-space steps; wait for a stable editor frame
    // after every move so the guest has consumed the full serial packet batch.
    vm.mouse_pin().unwrap();
    for points in [[(80, 100), (480, 100)], [(280, 30), (280, 160)]] {
        vm.type_line("LINE").unwrap();
        for (column, row) in points {
            let (x, y) = device_for_pixel(column, row);
            vm.mouse_to(x, y, 0).unwrap();
            vm.capture_editor().unwrap();
            vm.mouse_to(x, y, LEFT).unwrap();
            thread::sleep(Duration::from_millis(300));
            vm.mouse_to(x, y, 0).unwrap();
            thread::sleep(Duration::from_millis(300));
        }
        vm.type_line("").unwrap();
    }

    // Radius is remembered, but setting it exits FILLET to the command line.
    vm.type_line("FILLET").unwrap();
    vm.type_line("R").unwrap();
    vm.type_line("1").unwrap();
    vm.type_line("FILLET").unwrap();
    for (column, row) in [(140, 100), (280, 50)] {
        let (x, y) = device_for_pixel(column, row);
        vm.mouse_to(x, y, 0).unwrap();
        vm.capture_editor().unwrap();
        vm.mouse_to(x, y, LEFT).unwrap();
        thread::sleep(Duration::from_millis(300));
        vm.mouse_to(x, y, 0).unwrap();
        thread::sleep(Duration::from_millis(300));
    }
    vm.capture_editor().unwrap();
    // FILLET repeats its selection prompt after applying the operation.
    for _ in 0..2 {
        vm.key("esc", true).unwrap();
        vm.key("esc", false).unwrap();
        thread::sleep(Duration::from_millis(250));
    }
    vm.type_line("END").unwrap();
    vm.wait_for_text("Enter selection:", WAIT).unwrap();
    vm.shutdown().unwrap();

    let dwg = vm.read_system_file("FILLET1.DWG").unwrap();
    let drawing = acad_dwg::parse(&dwg).unwrap();
    let mut rust = acad_cmd::Editor::default();
    for input in [
        "LINE",
        "1.96078431372554,3.58823529411769",
        "11.7647058823534,3.58823529411769",
        "",
        "LINE",
        "6.86274509803926,7.70588235294122",
        "6.86274509803926,0.0588235294117652",
        "",
        "FILLET",
        "R",
        "1",
        "FILLET",
        "1,2",
    ] {
        rust.submit(input).unwrap();
    }
    use acad_model::{Entity, Item};
    assert_eq!(
        drawing.items.len(),
        3,
        "two trimmed lines and one fillet arc"
    );
    let line = |index| match &drawing.items[index] {
        Item::Entity(Entity::OnLayer { entity, .. }) => match entity.as_ref() {
            Entity::Line { start, end } => (*start, *end),
            other => panic!("expected LINE at item {index}, got {other:?}"),
        },
        other => panic!("expected entity at item {index}, got {other:?}"),
    };
    let (horizontal_start, horizontal_end) = line(0);
    let (vertical_start, vertical_end) = line(1);
    let arc = match &drawing.items[2] {
        Item::Entity(Entity::OnLayer { entity, .. }) => match entity.as_ref() {
            Entity::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            } => (*center, *radius, *start_deg, *end_deg),
            other => panic!("expected FILLET ARC, got {other:?}"),
        },
        other => panic!("expected FILLET entity, got {other:?}"),
    };
    assert_geometry_matches(&drawing.items, &rust.drawing().items);
    let close = |actual: f64, expected: f64| (actual - expected).abs() < 1e-9;
    // AutoCAD's selection order leaves the first line's forward arm and the
    // second line's backward arm. Their endpoints must meet the quarter-circle's
    // tangent points, proving FILLET changed the lines as well as adding arc.
    assert!(horizontal_end.x > 11.0, "horizontal far endpoint preserved");
    assert!(vertical_start.y > 7.0, "vertical far endpoint preserved");
    assert!(close(horizontal_start.x, arc.0.x));
    assert!(close(horizontal_start.y, arc.0.y - arc.1));
    assert!(close(vertical_end.x, arc.0.x - arc.1));
    assert!(close(vertical_end.y, arc.0.y));
    assert!(close(arc.1, 1.0), "requested FILLET radius");
    assert!(close(arc.2, 180.0) && close(arc.3, 270.0));
    assert!(close(vertical_start.x, vertical_end.x));
    assert!(close(horizontal_start.y, horizontal_end.y));
}
