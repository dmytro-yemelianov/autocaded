use acad_dwg::header::{parse_header, Version};

/// The corpus is extracted from archives that are deliberately not in git, so
/// a fresh checkout has none. Tests that need it skip rather than fail.
fn corpus(rel: &str) -> Option<Vec<u8>> {
    match std::fs::read(format!("../../corpus/{rel}")) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: corpus/{rel} absent (run ./tools/extract-corpus.sh)");
            None
        }
    }
}

/// SUBDIV.DXF prints doubles truncated to 6 decimal places (see the raw
/// header text: `8.500000,6.163380,16.326761`), while the DWG stores the
/// full 8-byte double — and that double carries ULP-level float noise from
/// whatever internal computation produced it (e.g. the DWG's raw TXTSIZE is
/// 0.19999999999999996, one ULP off the literal-0.2 double; DWGVIEW's height
/// is 16.326760563543544, not the clean value). A scan of the whole file for
/// the bit-exact "clean" doubles (8.5, 6.163380, 16.326761, 0.2, 0.05) finds
/// none anywhere, in or out of the header, so this is a real precision loss
/// in the DXF round-trip, not a mis-derived offset. Fields compare exactly
/// only where the underlying values happen to be exactly representable
/// (EXTENTS/LIMITS here are whole or `.75` values); the rest compare to
/// within the DXF's own rounding error.
const DXF_ROUNDING: f64 = 1e-6;

fn approx_eq(a: f64, b: f64, what: &str) {
    assert!(
        (a - b).abs() < DXF_ROUNDING,
        "{what}: dwg {a} vs dxf {b} (diff {})",
        (a - b).abs()
    );
}

#[test]
fn the_dwg_header_agrees_with_the_dxf_header() {
    let (Some(dwg), Some(dxf)) = (corpus("Samples/SUBDIV.DWG"), corpus("Samples/SUBDIV.DXF"))
    else {
        return;
    };
    let (from_dwg, _) = parse_header(&dwg).unwrap();
    let from_dxf = acad_dxf::parse(&dxf).unwrap().header;
    assert_eq!(from_dwg.extents, from_dxf.extents, "EXTENTS");
    assert_eq!(from_dwg.limits, from_dxf.limits, "LIMITS");
    approx_eq(from_dwg.view.center.x, from_dxf.view.center.x, "DWGVIEW x");
    approx_eq(from_dwg.view.center.y, from_dxf.view.center.y, "DWGVIEW y");
    approx_eq(from_dwg.view.height, from_dxf.view.height, "DWGVIEW height");
    assert_eq!(from_dwg.snap.on, from_dxf.snap.on, "MODERES on");
    approx_eq(
        from_dwg.snap.spacing,
        from_dxf.snap.spacing,
        "MODERES spacing",
    );
    assert_eq!(from_dwg.grid.on, from_dxf.grid.on, "MODEGRID on");
    approx_eq(
        from_dwg.grid.spacing,
        from_dxf.grid.spacing,
        "MODEGRID spacing",
    );
    // SUBDIV alone cannot distinguish the adjacent 1s at 0xb0 and 0xb2.
    // The generated FILL OFF oracle test pins MODEFILL to 0xb2.
    assert_eq!(from_dwg.ortho, from_dxf.ortho, "MODEORTHO");
    assert_eq!(from_dwg.fill, from_dxf.fill, "MODEFILL");
    approx_eq(from_dwg.text_size, from_dxf.text_size, "TXTSIZE");
    approx_eq(from_dwg.trace_width, from_dxf.trace_width, "TRACEWID");
    assert_eq!(from_dwg.base, from_dxf.base, "BASE");
    assert_eq!(from_dwg.current_layer, from_dxf.current_layer, "LAYER");
    assert_eq!(from_dwg.layers, from_dxf.layers, "LAYERC");
}

#[test]
fn the_entity_count_matches_what_the_dxf_holds() {
    // 171 = 159 entities + 6 BLOCK + 6 ENDBLK. Deriving it from the DXF rather
    // than hardcoding 171 is what makes this a check and not a restatement.
    let (Some(dwg), Some(dxf)) = (corpus("Samples/SUBDIV.DWG"), corpus("Samples/SUBDIV.DXF"))
    else {
        return;
    };
    let (_, meta) = parse_header(&dwg).unwrap();
    let drawing = acad_dxf::parse(&dxf).unwrap();
    // A Drawing is a flat Vec<Item> preserving document order; a Block counts
    // as its own entities plus the BLOCK and ENDBLK delimiters.
    let from_dxf = drawing.entities().count()
        + drawing
            .blocks()
            .map(|b| b.entities.len() + 2)
            .sum::<usize>();
    assert_eq!(meta.entity_count as usize, from_dxf);
}

#[test]
fn every_ac12_drawing_in_the_corpus_has_a_readable_header() {
    let mut seen = 0;
    for name in [
        "BOX", "FLOW", "ADDER", "FLOOR", "ORGATE", "DLATCH", "SUBDIV", "BLIVET", "ANDGATE",
        "NORGATE", "XORGATE", "HALFADD", "SELEXOL", "INVERTER", "NANDGATE", "XNORGATE",
    ] {
        let Some(bytes) = corpus(&format!("Samples/{name}.DWG")) else {
            return;
        };
        assert_eq!(Version::detect(&bytes).unwrap(), Version::Ac12, "{name}");
        let (_, meta) = parse_header(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        // A wrong field width (e.g. reading entity_count as u32 instead of
        // u16 — see spec §4.2) still returns Ok, just with nonsense inside;
        // "parses" alone doesn't catch that, so check the numbers are sane:
        // at least one entity, and no more entities than there are bytes to
        // hold them (each needs at least one byte).
        assert!(meta.entity_count > 0, "{name}: entity_count is 0");
        assert!(
            meta.entity_count < meta.entity_end,
            "{name}: entity_count {} >= entity_end {:#x}",
            meta.entity_count,
            meta.entity_end
        );
        seen += 1;
    }
    assert_eq!(seen, 16, "the census says 16 AC1.2 drawings");
}

#[test]
fn an_ac140_drawing_has_the_extended_header_and_correct_record_count() {
    let Some(bytes) = corpus("Samples/HOUSE.DWG") else {
        return;
    };
    assert_eq!(
        acad_dwg::header::Version::detect(&bytes).unwrap(),
        Version::Ac140
    );
    let (_, meta) = parse_header(&bytes).unwrap();
    assert_eq!(meta.version.entity_start(), 0x202);
    assert_eq!(meta.entity_end, 0x1613);
    assert_eq!(meta.entity_count, 141);
}
