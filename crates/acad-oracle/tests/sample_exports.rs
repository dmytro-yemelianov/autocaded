fn bare(mut entity: &acad_model::Entity) -> &acad_model::Entity {
    while let acad_model::Entity::OnLayer { entity: inner, .. } = entity {
        entity = inner;
    }
    entity
}

#[cfg(unix)]
#[test]
fn original_exports_disc_backup_with_font_loads_in_order() {
    use acad_model::Entity;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)");
    let system = root.join("System.img");
    let samples = root.join("Samples.img");
    if !system.exists() || !samples.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted floppy images or qemu-system-i386 absent");
        return;
    }
    let exports = acad_oracle::export_sample_backups(&system, &samples, &["DISC"]).unwrap();
    let dxf = acad_dxf::parse(&exports[0]).unwrap();
    let bytes = std::fs::read(root.join("../../Samples/DISC.BAK")).unwrap();
    let dwg = acad_dwg::parse(&bytes).unwrap();
    for drawing in [&dxf, &dwg] {
        let entities = drawing.entities().collect::<Vec<_>>();
        assert_eq!(entities.len(), 14);
        let entities = &entities[2..6];
        assert_eq!(
            *bare(entities[0]),
            Entity::Load {
                name: "ROMAN-S".into()
            }
        );
        assert!(matches!(bare(entities[1]), Entity::Insert { name, .. } if name == "SHUTTLE"));
        assert_eq!(
            *bare(entities[2]),
            Entity::Load {
                name: "ITALIC".into()
            }
        );
        assert!(
            matches!(bare(entities[3]), Entity::Text { value, height, .. } if value == "STAR WARS" && *height == 1.0)
        );
    }
    assert_eq!(
        acad_dxf::write(&ordered(dwg)),
        acad_dxf::write(&ordered(dxf))
    );
}

#[cfg(unix)]
#[test]
fn original_exports_verify_both_dwg_versions() {
    use acad_model::{Entity, Item};

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)");
    let system = root.join("System.img");
    let samples = root.join("Samples.img");
    if !system.exists() || !samples.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted floppy images or qemu-system-i386 absent");
        return;
    }

    // None has a DXF sibling on the original Samples disk. These are freshly
    // exported by AutoCAD on a copied disk, independent of the Rust codecs.
    let names = [
        "SELEXOL", "BLIVET", "FLOW", "FLOOR", "ADDER", "HOUSE", "COLORS", "OFFICE", "SHUTTLE",
    ];
    let exports = acad_oracle::export_samples(&system, &samples, &names).unwrap();
    for (name, dxf_bytes) in names.into_iter().zip(exports) {
        let records = acad_dxf::lex(&dxf_bytes).unwrap();
        let count = |keyword| records.iter().filter(|r| r.keyword == keyword).count();
        let repeat_pairs = match name {
            "BLIVET" => 5,
            "FLOOR" => 2,
            "SHUTTLE" => 1,
            _ => 0,
        };
        assert_eq!(count("REPEAT"), repeat_pairs, "{name}: repeat opens");
        assert_eq!(count("ENDREP"), repeat_pairs, "{name}: repeat closes");
        if name == "SELEXOL" {
            assert!(dxf_bytes.windows(9).any(|w| w == b"LINE,20\r\n"));
        }
        let dxf = acad_dxf::parse(&dxf_bytes)
            .unwrap_or_else(|e| panic!("{name}: original DXF did not parse: {e}"));
        let dwg_bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../corpus/Samples/{name}.DWG")),
        )
        .unwrap();
        let dwg = acad_dwg::parse(&dwg_bytes).unwrap();
        // The DWG block table is flat and can place definitions in a
        // different order from AutoCAD's export. Match blocks by name while
        // retaining entity order inside each block and among loose entities.
        // Compare the full header too, including BASE and the layer table.
        let expected = acad_dxf::write(&ordered(dxf.clone()));
        let actual = acad_dxf::write(&ordered(dwg));
        if actual != expected {
            let at = actual
                .iter()
                .zip(&expected)
                .position(|(a, b)| a != b)
                .unwrap_or(actual.len().min(expected.len()));
            panic!(
                "{name}: DWG differs from the original's exported DXF at byte {at}: {:?} vs {:?}",
                &actual[at..actual.len().min(at + 80)],
                &expected[at..expected.len().min(at + 80)]
            );
        }

        let all_entities = dxf.items.iter().flat_map(|item| match item {
            Item::Entity(e) => std::slice::from_ref(e).iter(),
            Item::Block(b) => b.entities.iter(),
            Item::Repeat(r) => r.entities.iter(),
        });
        let (mut points, mut traces, mut solids) = (0, 0, 0);
        for e in all_entities {
            let e = bare(e);
            match e {
                Entity::Point { .. } => points += 1,
                Entity::Trace { .. } => traces += 1,
                Entity::Solid { .. } => solids += 1,
                _ => {}
            }
        }
        let expected = match name {
            "SELEXOL" => Some((1, 18, 0)),
            "BLIVET" => Some((0, 16, 4)),
            "FLOW" => Some((0, 2, 2)),
            "FLOOR" | "ADDER" => Some((0, 0, 0)),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!((points, traces, solids), expected, "{name}: new DXF kinds");
        }
    }
}

#[cfg(unix)]
fn ordered(mut drawing: acad_model::Drawing) -> acad_model::Drawing {
    drawing.items.sort_by(|a, b| match (a, b) {
        (acad_model::Item::Block(a), acad_model::Item::Block(b)) => a.name.cmp(&b.name),
        (acad_model::Item::Block(_), _) => std::cmp::Ordering::Less,
        (_, acad_model::Item::Block(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    });
    drawing
}
