//! E2 explicit REPEAT owner layers in historical DXF
//! (docs/native-group-persistence.md, "E2"). The original's DXF has no owner
//! field: `REPEAT,<marker layer>`, members with their own layers, `ENDREP,<marker
//! layer>` (oracle `repeat_layer.rs`). An owner layer every member already
//! carries is dropped; any other is refused before output.
use acad_model::{Drawing, Entity, Item, Point, Repeat};

fn on(layer: u8, entity: Entity) -> Entity {
    Entity::OnLayer {
        layer,
        entity: Box::new(entity),
    }
}
fn line(y: f64) -> Entity {
    Entity::Line {
        start: Point { x: 1.0, y },
        end: Point { x: 2.0, y },
    }
}
fn group(start_layer: u8, end_layer: u8, entities: Vec<Entity>) -> Repeat {
    Repeat {
        start_layer,
        end_layer,
        entities,
        columns: 2,
        rows: 1,
        column_spacing: 5.0,
        row_spacing: 0.0,
    }
}
fn kept(nested_owner: bool, erased_member: bool) -> Repeat {
    let inner = Entity::Repeat(group(5, 6, vec![on(2, line(3.0))]));
    let mut entities = vec![on(2, line(1.0)), on(2, Entity::Load { name: "TXT".into() })];
    if erased_member {
        entities.push(Entity::Erased(Box::new(on(2, line(2.0)))));
    }
    entities.push(if nested_owner { on(2, inner) } else { inner });
    group(7, 9, entities)
}
fn drawing(items: Vec<Item>) -> Drawing {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,2\r\n").unwrap();
    drawing.items = items;
    drawing
}

#[test]
fn kept_owner_layer_exports_members_and_marker_layers_without_an_owner() {
    let source = drawing(vec![Item::Entity(on(2, Entity::Repeat(kept(true, true))))]);
    let bytes = acad_dxf::try_write(&source).unwrap();
    assert_eq!(acad_dxf::write(&source), bytes);
    assert_eq!(
        bytes,
        acad_dxf::try_write(&drawing(vec![Item::Repeat(kept(false, true))])).unwrap()
    );
    let text = String::from_utf8_lossy(&bytes);
    let records = &text[text.find("REPEAT,").unwrap()..];
    assert_eq!(
        records,
        "REPEAT,7\r\nLINE,2\r\n1.000000,1.000000,2.000000,1.000000\r\nLOAD,2\r\nTXT\r\n\
         REPEAT,5\r\nLINE,2\r\n1.000000,3.000000,2.000000,3.000000\r\n\
         ENDREP,6\r\n2,1,5.000000,0.000000\r\n\
         ENDREP,9\r\n2,1,5.000000,0.000000\r\n\u{1a}"
    );
    // Live-only exchange: the erased member is omitted, as for any group.
    assert_eq!(
        acad_dxf::parse(&bytes).unwrap().items,
        [Item::Repeat(kept(false, false))]
    );
}

#[test]
fn owner_layer_differing_from_any_member_is_refused_before_output() {
    for repeat in [
        group(2, 2, vec![on(2, line(1.0)), on(3, line(2.0))]),
        group(2, 2, vec![on(2, line(1.0)), line(2.0)]),
        // Even an erased member, which DXF would omit, must carry the layer.
        group(
            2,
            2,
            vec![on(2, line(1.0)), Entity::Erased(Box::new(on(3, line(2.0))))],
        ),
        group(
            2,
            2,
            vec![on(3, Entity::Repeat(group(2, 2, vec![on(3, line(1.0))])))],
        ),
    ] {
        let source = drawing(vec![Item::Entity(on(2, Entity::Repeat(repeat)))]);
        let error = acad_dxf::try_write(&source).unwrap_err();
        assert!(
            matches!(error, acad_dxf::DxfError::UnsupportedGroup { .. })
                && error.to_string().contains("owner layer"),
            "{error}"
        );
        assert!(std::panic::catch_unwind(|| acad_dxf::write(&source)).is_err());
    }
}
