#[cfg(unix)]
#[test]
fn original_exports_verify_ac12_entity_layouts() {
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
    let names = ["SELEXOL", "BLIVET", "FLOW", "FLOOR", "ADDER"];
    let exports = acad_oracle::export_samples(&system, &samples, &names).unwrap();
    for (name, dxf_bytes) in names.into_iter().zip(exports) {
        let records = acad_dxf::lex(&dxf_bytes).unwrap();
        let count = |keyword| records.iter().filter(|r| r.keyword == keyword).count();
        let repeat_pairs = match name {
            "BLIVET" => 5,
            "FLOOR" => 2,
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
        let mut dwg = acad_dwg::parse(&dwg_bytes).unwrap();
        // The DWG block table is flat and can place definitions in a
        // different order from AutoCAD's export. Match blocks by name while
        // retaining entity order inside each block and among loose entities.
        fn ordered(mut drawing: acad_model::Drawing) -> acad_model::Drawing {
            drawing.items.sort_by(|a, b| match (a, b) {
                (Item::Block(a), Item::Block(b)) => a.name.cmp(&b.name),
                (Item::Block(_), Item::Entity(_)) => std::cmp::Ordering::Less,
                (Item::Entity(_), Item::Block(_)) => std::cmp::Ordering::Greater,
                (Item::Entity(_), Item::Entity(_)) => std::cmp::Ordering::Equal,
            });
            drawing
        }
        // Header comparison is a separate problem; use the original DXF
        // header on both sides and compare canonical six-decimal fields.
        dwg.header = dxf.header.clone();
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
        });
        let (mut points, mut traces, mut solids) = (0, 0, 0);
        for e in all_entities {
            match e {
                Entity::Point { .. } => points += 1,
                Entity::Trace { .. } => traces += 1,
                Entity::Solid { .. } => solids += 1,
                _ => {}
            }
        }
        let expected = match name {
            "SELEXOL" => (1, 18, 0),
            "BLIVET" => (0, 16, 4),
            "FLOW" => (0, 2, 2),
            _ => (0, 0, 0),
        };
        assert_eq!((points, traces, solids), expected, "{name}: new DXF kinds");
    }
}
