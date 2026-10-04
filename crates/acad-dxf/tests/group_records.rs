use acad_model::{Entity, Item};
#[test]
fn nested_marker_layers_are_metadata_and_preserve_order_and_six_decimals() {
    let source = b"REPEAT,7\r\nPOINT,3\r\n1.25,2.5\r\nREPEAT,4\r\nLOAD,6\r\nFONT\r\nPOINT,5\r\n-1,3\r\nENDREP,8\r\n3,2,-10,5\r\nENDREP,9\r\n2,1,2,0\r\n";
    let drawing = acad_dxf::parse(source).unwrap();
    let Item::Repeat(outer) = &drawing.items[0] else {
        panic!("pattern");
    };
    assert_eq!((outer.start_layer, outer.end_layer), (7, 9));
    assert!(matches!(
        &outer.entities[0],
        Entity::OnLayer { layer: 3, .. }
    ));
    let Entity::Repeat(inner) = &outer.entities[1] else {
        panic!("inner pattern");
    };
    assert_eq!((inner.start_layer, inner.end_layer), (4, 8));
    let written = acad_dxf::try_write(&drawing).unwrap();
    assert!(String::from_utf8(written.clone())
        .unwrap()
        .contains("POINT,3\r\n1.250000,2.500000\r\nREPEAT,4\r\nLOAD,6\r\nFONT"));
    assert_eq!(acad_dxf::parse(&written).unwrap().items, drawing.items);
    assert_eq!(acad_dxf::write(&drawing), written);
}
#[test]
fn groups_cannot_cross_block_boundaries_or_exceed_depth() {
    for source in [
        "REPEAT,1\r\nPOINT,1\r\n1,2\r\nBLOCK,1\r\n0,0\r\nB\r\nENDREP,1\r\n1,1,0,0\r\nENDBLK,1\r\n",
        "BLOCK,1\r\n0,0\r\nB\r\nREPEAT,1\r\nPOINT,1\r\n1,2\r\nENDBLK,1\r\nENDREP,1\r\n1,1,0,0\r\n",
        // An empty pair is valid (R6, tests/empty_repeat.rs); zero
        // dimensions are not.
        "REPEAT,1\r\nENDREP,1\r\n0,1,0,0\r\n",
        "REPEAT,256\r\nPOINT,1\r\n1,2\r\nENDREP,1\r\n1,1,0,0\r\n",
        "REPEAT,1\r\nPOINT,1\r\n1,2\r\nENDREP,1\r\n1,1,NaN,0\r\n",
        "REPEAT,1\r\nREPEAT,1\r\nPOINT,1\r\n1,2\r\nENDREP,1\r\n1,1,0,0\r\n",
    ] {
        assert!(acad_dxf::parse(source.as_bytes()).is_err());
    }
    assert!(acad_dxf::parse("REPEAT,1\r\n".repeat(65).as_bytes())
        .unwrap_err()
        .to_string()
        .contains("limit of 64"));
}
#[test]
fn nested_missing_insert_dependency_is_rejected() {
    let source = b"REPEAT,1\r\nREPEAT,2\r\nINSERT,3\r\n1,2,1,1,0\r\nMISSING\r\nENDREP,2\r\n1,1,0,0\r\nENDREP,1\r\n1,1,0,0\r\n";
    assert!(matches!(
        acad_dxf::parse(source),
        Err(acad_dxf::DxfError::UndefinedBlock { .. })
    ));
}
