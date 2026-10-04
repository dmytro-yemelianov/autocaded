//! Writer safety contracts; no original-program parity is asserted.
use acad_model::{Block, Drawing, Entity, Item, Point, Repeat};
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn group(entities: Vec<Entity>) -> Repeat {
    Repeat {
        start_layer: 1,
        end_layer: 1,
        entities,
        columns: 2,
        rows: 1,
        column_spacing: 3.0,
        row_spacing: 0.0,
    }
}
fn flat() -> Repeat {
    group(vec![Entity::Point {
        origin: p(1.0, 2.0),
    }])
}
fn layer(entity: Entity) -> Entity {
    Entity::OnLayer {
        layer: 2,
        entity: Box::new(entity),
    }
}
fn drawing(items: Vec<Item>) -> Drawing {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,2\r\n").unwrap();
    drawing.items = items;
    drawing
}

#[test]
fn nested_groups_are_supported_but_explicit_owners_are_refused() {
    let outer = group(vec![Entity::Repeat(flat())]);
    for item in [
        Item::Repeat(outer.clone()),
        Item::Entity(Entity::Repeat(outer.clone())),
        Item::Block(Block {
            name: "B".into(),
            base: p(0.0, 0.0),
            entities: vec![Entity::Repeat(outer.clone())],
        }),
    ] {
        let source = drawing(vec![item]);
        for version in [
            acad_dwg::header::Version::Ac12,
            acad_dwg::header::Version::Ac140,
        ] {
            for bytes in [
                acad_dwg::write_version(&source, version).unwrap(),
                acad_dwg::write::encode_version(&source, version).unwrap(),
            ] {
                let reopened = acad_dwg::parse(&bytes).unwrap();
                assert_eq!(acad_dwg::write_version(&reopened, version).unwrap(), bytes);
            }
        }
    }
    // Owner layer 2 over bare (layer 1) members: no file keeps it (E2,
    // `owner_layer.rs` covers the owners a file does keep).
    for item in [
        Item::Entity(layer(Entity::Repeat(outer.clone()))),
        Item::Erased(layer(Entity::Repeat(outer))),
        Item::Entity(layer(layer(Entity::Point {
            origin: p(0.0, 0.0),
        }))),
    ] {
        let source = drawing(vec![item]);
        for version in [
            acad_dwg::header::Version::Ac12,
            acad_dwg::header::Version::Ac140,
        ] {
            for result in [
                acad_dwg::write_version(&source, version),
                acad_dwg::write::encode_version(&source, version),
            ] {
                assert!(matches!(
                    result,
                    Err(acad_dwg::DwgError::WriteValue {
                        field: "REPEAT",
                        ..
                    })
                ));
            }
        }
        assert!(acad_dwg::write(&source).is_err());
        assert!(acad_dwg::write::encode(&source).is_err());
    }
    // Refuse before recursive serialization, without expanding any repeat cell.
    let mut entity = Entity::Point {
        origin: p(0.0, 0.0),
    };
    for _ in 0..256 {
        entity = Entity::Repeat(group(vec![entity]));
    }
    let source = drawing(vec![Item::Entity(entity)]);
    assert!(acad_dwg::write(&source)
        .unwrap_err()
        .to_string()
        .contains("limit of 64"));
    assert!(acad_dwg::write_version(&source, acad_dwg::header::Version::Ac12).is_err());
}

#[test]
fn flat_repeats_in_blocks_and_insert_references_remain_writable() {
    let source = drawing(vec![
        Item::Repeat(flat()),
        Item::Block(Block {
            name: "B".into(),
            base: p(0.0, 0.0),
            entities: vec![Entity::Repeat(flat())],
        }),
        Item::Entity(Entity::Repeat(group(vec![Entity::Insert {
            origin: p(10.0, 0.0),
            x_scale: 1.0,
            y_scale: 1.0,
            rotation_deg: 0.0,
            name: "B".into(),
        }]))),
    ]);
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let reopened =
            acad_dwg::parse(&acad_dwg::write_version(&source, version).unwrap()).unwrap();
        assert_eq!(
            reopened
                .items
                .iter()
                .filter(|item| matches!(item, Item::Repeat(_)))
                .count(),
            2
        );
        let block = reopened.block("B").unwrap();
        assert!(matches!(&block.entities[0], Entity::Repeat(repeat) if repeat.columns == 2));
    }
}
