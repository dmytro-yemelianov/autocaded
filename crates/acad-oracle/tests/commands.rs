#[cfg(unix)]
#[test]
fn original_creates_circles_with_the_requested_centers_and_radii() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }

    let cases = [(4.0, 3.0, 2.0), (7.25, 5.5, 1.25)];
    let mut lines = Vec::new();
    let mut expected = Vec::new();
    for (x, y, radius) in cases {
        lines.extend(["CIRCLE".to_owned(), format!("{x},{y}"), radius.to_string()]);
        expected.push(Item::Entity(Entity::Circle {
            center: Point { x, y },
            radius,
        }));
    }
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (dwg, dxf) = acad_oracle::generate_pair(&disk, "ORCCIRC", &lines).unwrap();
    assert_eq!(&dwg[..6], b"AC1.40");
    let mut drawing = acad_dxf::parse(&dxf).unwrap();
    assert_eq!(drawing.items, expected, "original CIRCLE command geometry");

    // Expected entities come from the requested inputs. Preserve only the
    // original's header, then compare our writer with its actual DXF bytes.
    drawing.items = expected;
    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&drawing), dxf[..end]);
}

#[cfg(unix)]
#[test]
fn original_creates_a_line_and_a_three_point_arc() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    // ACAD.HLP documents RETURN to terminate LINE and three points for ARC.
    let (_, dxf) = acad_oracle::generate_pair(
        &disk,
        "ORCARC",
        &[
            "LINE", "1.25,2.5", "9.5,6.75", "", "ARC", "4,3", "3,4", "2,3",
        ],
    )
    .unwrap();
    let drawing = acad_dxf::parse(&dxf).unwrap();
    assert_eq!(
        drawing.items,
        vec![
            Item::Entity(Entity::Line {
                start: Point { x: 1.25, y: 2.5 },
                end: Point { x: 9.5, y: 6.75 },
            }),
            // The three input points define the upper half of the unit
            // circle around (3, 3), counterclockwise from (4, 3) to (2, 3).
            Item::Entity(Entity::Arc {
                center: Point { x: 3.0, y: 3.0 },
                radius: 1.0,
                start_deg: 0.0,
                end_deg: 180.0,
            }),
        ],
    );
    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&drawing), dxf[..end]);
}
