use acad_model::{Entity, Item};
fn erase(entity: Entity) -> Entity {
    Entity::Erased(Box::new(entity))
}
#[test]
fn live_only_export_skips_erased_members_keeps_live_group_markers_and_load_content() {
    let mut drawing = acad_dxf::parse(b"REPEAT,7\r\nREPEAT,8\r\nPOINT,2\r\n1,2\r\nENDREP,9\r\n2,1,3,0\r\nENDREP,10\r\n2,1,4,0\r\nBLOCK,1\r\n0,0\r\nB\r\nREPEAT,11\r\nLOAD,3\r\nALT\r\nPOINT,4\r\n5,6\r\nENDREP,12\r\n2,1,5,0\r\nENDBLK,1\r\n").unwrap();
    let Item::Repeat(outer) = &mut drawing.items[0] else {
        panic!("outer")
    };
    let Entity::Repeat(inner) = &mut outer.entities[0] else {
        panic!("inner")
    };
    inner.entities[0] = erase(inner.entities[0].clone());
    let Item::Block(block) = &mut drawing.items[1] else {
        panic!("block")
    };
    let Entity::Repeat(r) = &mut block.entities[0] else {
        panic!("load group")
    };
    r.entities[1] = erase(r.entities[1].clone());
    let before = drawing.clone();
    let bytes = acad_dxf::try_write(&drawing).unwrap();
    assert_eq!(drawing, before);
    assert_eq!(acad_dxf::write(&drawing), bytes);
    let parsed = acad_dxf::parse(&bytes).unwrap();
    // R6: live groups keep their markers without live members, as AutoCAD
    // 1.4's task 5 writes them; the erased POINT is skipped.
    assert_eq!(parsed.items.len(), 2);
    let Item::Repeat(outer) = &parsed.items[0] else {
        panic!("outer kept")
    };
    assert_eq!(
        (outer.start_layer, outer.end_layer, outer.column_spacing),
        (7, 10, 4.0)
    );
    let [Entity::Repeat(inner)] = outer.entities.as_slice() else {
        panic!("inner kept")
    };
    assert_eq!((inner.start_layer, inner.end_layer), (8, 9));
    assert!(
        inner.entities.is_empty(),
        "the erased POINT is not exported"
    );
    let Entity::Repeat(r) = &parsed.block("B").unwrap().entities[0] else {
        panic!("retained LOAD group")
    };
    assert_eq!((r.start_layer, r.end_layer), (11, 12));
    assert_eq!(r.entities.len(), 1);
    assert!(
        matches!(&r.entities[0], Entity::OnLayer { layer: 3, entity } if matches!(entity.as_ref(), Entity::Load { name } if name == "ALT"))
    );
    assert_eq!(acad_dxf::try_write(&parsed).unwrap(), bytes);
    let Item::Repeat(r) = drawing.items.remove(0) else {
        unreachable!()
    };
    drawing.items.insert(0, Item::Erased(Entity::Repeat(r)));
    // Whole erased owners omit prior member history intentionally in DXF,
    // whereas retained DWG history must refuse that ambiguous state.
    let omitted = acad_dxf::try_write(&drawing).unwrap();
    let mut without_owner = drawing.clone();
    without_owner.items.remove(0);
    assert_eq!(omitted, acad_dxf::try_write(&without_owner).unwrap());
    assert_eq!(acad_dxf::parse(&omitted).unwrap().items.len(), 1);
}
#[test]
fn erased_fields_wrappers_and_depth_remain_checked_even_when_omitted() {
    let mut drawing =
        acad_dxf::parse(b"REPEAT,1\r\nPOINT,2\r\n1,2\r\nENDREP,1\r\n2,1,3,0\r\n").unwrap();
    let Item::Repeat(r) = drawing.items.remove(0) else {
        unreachable!()
    };
    let leaf = r.entities[0].clone();
    for malformed in [
        erase(erase(leaf)),
        erase(Entity::Repeat(r.clone())),
        erase(Entity::Point {
            origin: acad_model::Point {
                x: f64::NAN,
                y: 0.0,
            },
        }),
    ] {
        let mut group = r.clone();
        group.entities = vec![malformed];
        drawing.items = vec![Item::Repeat(group.clone())];
        assert!(acad_dxf::try_write(&drawing).is_err());
        assert!(std::panic::catch_unwind(|| acad_dxf::write(&drawing)).is_err());
        drawing.items = vec![Item::Erased(Entity::Repeat(group))];
        assert!(acad_dxf::try_write(&drawing).is_err());
    }
    let mut entity = erase(r.entities[0].clone());
    for _ in 0..65 {
        let mut group = r.clone();
        group.entities = vec![entity];
        entity = Entity::Repeat(group);
    }
    drawing.items = vec![Item::Erased(entity)];
    assert!(acad_dxf::try_write(&drawing)
        .unwrap_err()
        .to_string()
        .contains("limit of 64"));
}
