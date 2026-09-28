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
    let from_dwg = acad_dwg::parse(&dwg).unwrap();
    assert_eq!(acad_dxf::write(&from_dwg), dxf[..end]);
    let (_, meta) = acad_dwg::header::parse_header(&dwg).unwrap();
    for cut in 0..meta.entity_end as usize {
        assert!(
            acad_dwg::parse(&dwg[..cut]).is_err(),
            "accepted truncated DWG at {cut:#x}"
        );
    }
}

#[cfg(unix)]
#[test]
fn original_header_settings_survive_binary_decoding() {
    use acad_model::{Extents, Mode, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let (dwg, dxf) = acad_oracle::generate_pair(
        &disk,
        "ORCHEAD",
        &[
            "BASE", "2.25,3.5", "SNAP", "0.375", "GRID", "0.625", "ORTHO", "ON", "FILL", "OFF",
            "LIMITS", "-1,-2", "14,11", "LAYER", "12", "COLOR", "5", "3", "COLOR", "2", "",
        ],
    )
    .unwrap();
    let drawing = acad_dxf::parse(&dxf).unwrap();
    let h = &drawing.header;
    assert_eq!(h.base, Point { x: 2.25, y: 3.5 });
    assert_eq!(
        h.limits,
        Extents {
            xmin: -1.0,
            xmax: 14.0,
            ymin: -2.0,
            ymax: 11.0
        }
    );
    assert_eq!(
        h.snap,
        Mode {
            on: true,
            spacing: 0.375
        }
    );
    assert_eq!(
        h.grid,
        Mode {
            on: true,
            spacing: 0.625
        }
    );
    assert!(h.ortho);
    assert!(!h.fill);
    assert_eq!(h.current_layer, 3);
    assert_eq!(h.layers.keys().copied().collect::<Vec<_>>(), [0, 1, 3, 12]);
    assert_eq!(h.layers[&3], 2);
    assert_eq!(h.layers[&12], 5);
    assert!(drawing.items.is_empty());

    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&acad_dwg::parse(&dwg).unwrap()), dxf[..end]);
}

#[cfg(unix)]
#[test]
fn original_creates_a_line_arc_and_rotated_text() {
    use acad_model::{Entity, Item, Point};

    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    // ACAD.HLP documents RETURN to terminate LINE and three points for ARC.
    let (dwg, dxf) = acad_oracle::generate_pair(
        &disk,
        "ORCARC",
        &[
            "LINE", "1.25,2.5", "9.5,6.75", "", "ARC", "4,3", "3,4", "2,3", "TEXT", "2.25,3.5",
            "0.75", "30", "ORACLE",
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
            Item::Entity(Entity::Text {
                origin: Point { x: 2.25, y: 3.5 },
                height: 0.75,
                rotation_deg: 30.0,
                value: "ORACLE".to_owned(),
            }),
        ],
    );
    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&drawing), dxf[..end]);
    let from_dwg = acad_dwg::parse(&dwg).unwrap();
    assert_eq!(acad_dxf::write(&from_dwg), dxf[..end]);
}
