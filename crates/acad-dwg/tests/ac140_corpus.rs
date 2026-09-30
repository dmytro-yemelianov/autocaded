use acad_dwg::header::{parse_header, Version};
use acad_model::{Drawing, Entity, Item};
use acad_render::{flatten, rasterize, Viewport};

const MIN_LIT_PIXELS: usize = 500;

fn insert_depth(drawing: &Drawing, entity: &Entity, path: &mut Vec<String>) -> usize {
    match entity {
        Entity::OnLayer { entity, .. } => insert_depth(drawing, entity, path),
        Entity::Repeat(repeat) => repeat
            .entities
            .iter()
            .map(|entity| insert_depth(drawing, entity, path))
            .max()
            .unwrap_or(0),
        Entity::Insert { name, .. } => {
            if path.iter().any(|entry| entry.eq_ignore_ascii_case(name)) {
                return 0;
            }
            let Some(block) = drawing.block(name) else {
                return 0;
            };
            path.push(name.clone());
            let nested = block
                .entities
                .iter()
                .map(|entity| insert_depth(drawing, entity, path))
                .max()
                .unwrap_or(0);
            path.pop();
            1 + nested
        }
        _ => 0,
    }
}

fn max_top_level_insert_depth(drawing: &Drawing) -> usize {
    drawing
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Entity(entity) => Some(insert_depth(drawing, entity, &mut Vec::new())),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

#[test]
fn supported_ac140_corpus_drawings_and_backups_parse_and_render() {
    let files = [
        "HOUSE.DWG",
        "HOUSE.BAK",
        "COLORS.DWG",
        "COLORS.BAK",
        "OFFICE.DWG",
        "OFFICE.BAK",
        "SHUTTLE.DWG",
        "DISC.BAK",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/Samples");
    if !root.exists() {
        eprintln!("skipping: corpus absent (run ./tools/extract-corpus.sh)");
        return;
    }
    for name in files {
        let bytes = std::fs::read(root.join(name)).unwrap();
        let (_, meta) = parse_header(&bytes).unwrap();
        assert_eq!(meta.version, Version::Ac140, "{name}");
        let drawing = acad_dwg::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let vp = Viewport::fit(&drawing.header.limits, 800, 800);
        let pixmap = rasterize(&flatten(&drawing, &vp), 800, 800);
        let lit = pixmap
            .pixels()
            .iter()
            .filter(|p| p.red() > 0 || p.green() > 0 || p.blue() > 0)
            .count();
        eprintln!("{name}: {} records, {lit} lit pixels", meta.entity_count);
        assert!(
            lit > MIN_LIT_PIXELS,
            "{name}: rendered near-blank ({lit} lit pixels, minimum {MIN_LIT_PIXELS})"
        );
    }
}

#[test]
fn all_ac12_sample_drawings_parse_and_render_including_nested_insert_cases() {
    let files = [
        "ADDER.DWG",
        "ANDGATE.DWG",
        "BLIVET.DWG",
        "BOX.DWG",
        "DLATCH.DWG",
        "FLOOR.DWG",
        "FLOW.DWG",
        "HALFADD.DWG",
        "INVERTER.DWG",
        "NANDGATE.DWG",
        "NORGATE.DWG",
        "ORGATE.DWG",
        "SELEXOL.DWG",
        "SUBDIV.DWG",
        "XNORGATE.DWG",
        "XORGATE.DWG",
    ];
    assert_eq!(
        files.len(),
        16,
        "AC1.2 sample corpus coverage must stay 16/16"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/Samples");
    if !root.exists() {
        eprintln!("skipping: corpus absent (run ./tools/extract-corpus.sh)");
        return;
    }
    for name in files {
        let bytes = std::fs::read(root.join(name)).unwrap();
        let (_, meta) = parse_header(&bytes).unwrap();
        assert_eq!(meta.version, Version::Ac12, "{name}");
        let drawing = acad_dwg::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        if matches!(name, "BLIVET.DWG" | "SELEXOL.DWG") {
            assert!(
                max_top_level_insert_depth(&drawing) >= 2,
                "{name}: expected a top-level insert path nested at least two references deep"
            );
        }
        let vp = Viewport::fit(&drawing.header.limits, 800, 800);
        let primitives = flatten(&drawing, &vp);
        let pixmap = rasterize(&primitives, 800, 800);
        let lit = pixmap
            .pixels()
            .iter()
            .filter(|p| p.red() > 0 || p.green() > 0 || p.blue() > 0)
            .count();
        eprintln!(
            "{name}: {} records, {} primitives, {lit} lit pixels",
            meta.entity_count,
            primitives.len()
        );
        assert!(
            lit > MIN_LIT_PIXELS,
            "{name}: rendered near-blank ({lit} lit pixels, minimum {MIN_LIT_PIXELS})"
        );
    }
}
