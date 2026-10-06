use acad_model::{Block, Entity, Item, Point, Repeat};

fn text(value: &str) -> Entity {
    Entity::Text {
        origin: Point { x: 0.0, y: 0.0 },
        height: 1.0,
        rotation_deg: 0.0,
        value: value.into(),
    }
}

#[test]
fn text_controls_are_rejected_without_rejecting_unicode() {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,1\r\n").unwrap();
    for value in [
        "first\r\nsecond",
        "a\nb",
        "a\rb",
        "a\u{1a}b",
        "a\0b",
        "a\tb",
    ] {
        drawing.items = vec![Item::Entity(text(value))];
        assert_eq!(
            acad_dxf::try_write(&drawing).unwrap_err(),
            acad_dxf::DxfError::InvalidString {
                field: "TEXT value"
            }
        );
    }
    drawing.items = vec![Item::Entity(Entity::OnLayer {
        layer: 1,
        entity: Box::new(text("Привіт, café!")),
    })];
    let bytes = acad_dxf::try_write(&drawing).unwrap();
    assert_eq!(acad_dxf::parse(&bytes).unwrap().items, drawing.items);
}

#[test]
fn nested_live_strings_are_checked_but_erased_strings_are_omitted() {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,1\r\n").unwrap();
    let repeat = Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![text("a\u{1a}b")],
        columns: 2,
        rows: 1,
        column_spacing: 1.0,
        row_spacing: 0.0,
    };
    drawing.items = vec![Item::Repeat(repeat.clone())];
    assert!(acad_dxf::try_write(&drawing).is_err());
    drawing.items = vec![Item::Block(Block {
        name: "block".into(),
        base: Point { x: 0.0, y: 0.0 },
        entities: vec![Entity::Repeat(repeat)],
    })];
    assert!(acad_dxf::try_write(&drawing).is_err());
    drawing.items = vec![Item::Block(Block {
        name: "block".into(),
        base: Point { x: 0.0, y: 0.0 },
        entities: vec![Entity::Erased(Box::new(text("a\u{1a}b")))],
    })];
    assert!(acad_dxf::parse(&acad_dxf::try_write(&drawing).unwrap()).is_ok());
}

#[test]
fn names_and_custom_rows_cannot_inject_dxf_records() {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,1\r\n").unwrap();
    for entity in [
        Entity::Load {
            name: "TXT\r\nPOINT,1\r\n1,1".into(),
        },
        Entity::Insert {
            origin: Point { x: 0.0, y: 0.0 },
            x_scale: 1.0,
            y_scale: 1.0,
            rotation_deg: 0.0,
            name: "block\u{1a}".into(),
        },
        Entity::Generic(acad_model::GenericEntity {
            type_name: "POINT".into(),
            layer: 1,
            rows: vec!["1,1\r\nPOINT,1\r\n2,2".into()],
        }),
    ] {
        drawing.items = vec![Item::Entity(entity)];
        assert!(matches!(
            acad_dxf::try_write(&drawing),
            Err(acad_dxf::DxfError::InvalidString { .. })
        ));
    }
    drawing.items = vec![Item::Block(Block {
        name: "block\r\nENDBLK,1".into(),
        base: Point { x: 0.0, y: 0.0 },
        entities: vec![],
    })];
    assert_eq!(
        acad_dxf::try_write(&drawing).unwrap_err(),
        acad_dxf::DxfError::InvalidString {
            field: "BLOCK name"
        }
    );
}
