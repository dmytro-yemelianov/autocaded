use acad_dwg::header::{parse_header, Version};
use acad_render::{flatten, rasterize, Viewport};

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
        assert!(lit > 500, "{name}: rendered near-blank ({lit} lit pixels)");
    }
}

#[test]
fn disc_reports_its_unimplemented_font_load() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/Samples/DISC.BAK");
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    // A real LOAD header followed by the length-prefixed name ROMAN-S.
    assert_eq!(&bytes[0x1ca3..0x1cb0], b"\x0a\0\x01\0\x07\0ROMAN-S");
    assert_eq!(
        acad_dwg::parse(&bytes).unwrap_err(),
        acad_dwg::DwgError::UnknownEntityType {
            code: 10,
            at: 0x1ca3
        }
    );
}
