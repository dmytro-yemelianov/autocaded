fn corpus(rel: &str) -> Option<Vec<u8>> {
    match std::fs::read(format!("../../corpus/{rel}")) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: corpus/{rel} absent (run ./tools/extract-corpus.sh)");
            None
        }
    }
}

#[test]
fn subdiv_decodes_to_the_same_drawing_from_either_format() {
    let (Some(dwg), Some(dxf)) = (corpus("Samples/SUBDIV.DWG"), corpus("Samples/SUBDIV.DXF"))
    else {
        return;
    };
    let from_dwg = acad_dwg::parse(&dwg).unwrap();
    let from_dxf = acad_dxf::parse(&dxf).unwrap();

    // Review Focus 5: two empty drawings are equal, and a reader that returns
    // nothing would otherwise pass the milestone.
    //
    // 78, not 159: `Drawing::entities()` is deliberately non-recursive (it
    // yields only top-level `Item::Entity`s, not a block's interior
    // entities — see acad_model::drawing's doc comment). 159 is the flat
    // total of entity-shaped *records* (78 loose + 81 inside the 6 blocks:
    // the 2 CIRCLEs in TREE/HYDRANT plus ~79 LINEs across the 4 house
    // blocks), which is what `read_entities`/`entity_count` count (Task 5).
    // acad-dxf's own oracle test (crates/acad-dxf/tests/subdiv.rs) already
    // establishes "78 loose entities + 6 block definitions" as SUBDIV's
    // ground truth, so that is the number checked here.
    assert_eq!(
        from_dxf.entities().count(),
        78,
        "the DXF oracle itself must be whole"
    );
    assert_eq!(from_dxf.blocks().count(), 6);

    assert_eq!(
        from_dwg.entities().count(),
        from_dxf.entities().count(),
        "entity count"
    );
    assert_eq!(
        from_dwg.blocks().count(),
        from_dxf.blocks().count(),
        "block count"
    );

    // Review Focus 4: items, not entities. Comparing the flat item list is what
    // catches an ordering difference, because a Block in the wrong place is
    // still the right Block.
    assert_eq!(from_dwg.items.len(), from_dxf.items.len(), "item count");
    for (i, (a, b)) in from_dwg.items.iter().zip(from_dxf.items.iter()).enumerate() {
        assert_eq!(
            std::mem::discriminant(a),
            std::mem::discriminant(b),
            "item {i} is a different kind — document order differs between the formats"
        );
    }
    assert_eq!(
        from_dwg
            .blocks()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>(),
        from_dxf
            .blocks()
            .map(|b| b.name.as_str())
            .collect::<Vec<_>>(),
        "block names, in order"
    );
}
