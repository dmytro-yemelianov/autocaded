//! E2 explicit REPEAT owner layers (docs/native-group-persistence.md, "E2").
//! AutoCAD 1.4 keeps a group's layer only in its member records: CHANGE layer
//! rewrites the members and never the markers (oracle `repeat_layer.rs`). An
//! explicit owner layer is written exactly when every member already carries
//! it; the owner is dropped and the markers keep their own layers. Any other
//! owner layer is refused before output.
use acad_dwg::header::Version;
use acad_model::group_codec::{has_ambiguous_owner, original_member_erasure};
use acad_model::{Block, Drawing, Entity, Item, Point, Repeat};

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
fn erased(entity: Entity) -> Entity {
    Entity::Erased(Box::new(entity))
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
/// Every stored member on layer 2, through a kept nested owner, a bare nested
/// group, a LOAD and an erased member. Marker layers differ on purpose.
fn kept(nested_owner: bool) -> Repeat {
    let inner = Entity::Repeat(group(5, 6, vec![on(2, line(3.0))]));
    group(
        7,
        9,
        vec![
            on(2, line(1.0)),
            on(2, Entity::Load { name: "TXT".into() }),
            erased(on(2, line(2.0))),
            if nested_owner { on(2, inner) } else { inner },
            Entity::Repeat(group(8, 8, vec![on(2, line(4.0))])),
        ],
    )
}
fn drawing(items: Vec<Item>) -> Drawing {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,2\r\n").unwrap();
    drawing.items = items;
    drawing
}
fn writes(source: &Drawing, version: Version) -> Result<Vec<u8>, String> {
    let checked = acad_dwg::write_version(source, version).map_err(|e| e.to_string());
    let encoded = acad_dwg::write::encode_version(source, version).map_err(|e| e.to_string());
    assert_eq!(checked, encoded);
    checked
}

#[test]
fn kept_owner_layer_writes_the_members_and_markers_unchanged() {
    let canonical = drawing(vec![Item::Repeat(kept(false))]);
    for source in [
        drawing(vec![Item::Entity(on(2, Entity::Repeat(kept(true))))]),
        drawing(vec![Item::Entity(on(2, Entity::Repeat(kept(false))))]),
        drawing(vec![Item::Repeat(kept(true))]),
    ] {
        for version in [Version::Ac12, Version::Ac140] {
            let bytes = writes(&source, version).unwrap();
            assert_eq!(bytes, writes(&canonical, version).unwrap(), "{version:?}");
            let reopened = acad_dwg::parse(&bytes).unwrap();
            assert_eq!(reopened.items, canonical.items);
            let Item::Repeat(group) = &reopened.items[0] else {
                panic!("group")
            };
            // Marker layers are metadata: never replaced by the owner layer.
            assert_eq!((group.start_layer, group.end_layer), (7, 9));
        }
    }
    // Layer 1 owner over bare members (the default record layer).
    let bare = group(1, 1, vec![line(1.0)]);
    let source = drawing(vec![Item::Entity(on(1, Entity::Repeat(bare.clone())))]);
    for version in [Version::Ac12, Version::Ac140] {
        assert_eq!(
            writes(&source, version).unwrap(),
            writes(&drawing(vec![Item::Repeat(bare.clone())]), version).unwrap()
        );
    }
    // Inside a block definition too.
    let block = |entity| {
        drawing(vec![Item::Block(Block {
            name: "B".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![entity],
        })])
    };
    for version in [Version::Ac12, Version::Ac140] {
        assert_eq!(
            writes(&block(on(2, Entity::Repeat(kept(true)))), version).unwrap(),
            writes(&block(Entity::Repeat(kept(false))), version).unwrap()
        );
    }
}

#[test]
fn owner_layer_differing_from_any_member_is_refused_before_output() {
    let mismatches: Vec<Repeat> = vec![
        // An ordinary member on another layer.
        group(2, 2, vec![on(2, line(1.0)), on(3, line(2.0))]),
        // A bare member: layer 1.
        group(2, 2, vec![on(2, line(1.0)), line(2.0)]),
        // A LOAD member on another layer.
        group(
            2,
            2,
            vec![on(2, line(1.0)), Entity::Load { name: "TXT".into() }],
        ),
        // An erased member on another layer.
        group(2, 2, vec![on(2, line(1.0)), erased(on(3, line(2.0)))]),
        // A nested owner layer, even one its own members repeat.
        group(
            2,
            2,
            vec![on(3, Entity::Repeat(group(2, 2, vec![on(3, line(1.0))])))],
        ),
        // A member deep in a bare nested group.
        group(
            2,
            2,
            vec![Entity::Repeat(group(
                2,
                2,
                vec![Entity::Repeat(group(2, 2, vec![on(4, line(1.0))]))],
            ))],
        ),
    ];
    for repeat in mismatches {
        for item in [
            Item::Entity(on(2, Entity::Repeat(repeat.clone()))),
            Item::Repeat(group(1, 1, vec![on(2, Entity::Repeat(repeat.clone()))])),
            Item::Block(Block {
                name: "B".into(),
                base: Point { x: 0.0, y: 0.0 },
                entities: vec![on(2, Entity::Repeat(repeat.clone()))],
            }),
        ] {
            let source = drawing(vec![item]);
            for version in [Version::Ac12, Version::Ac140] {
                let error = writes(&source, version).unwrap_err();
                assert!(error.contains("owner layer"), "{error}");
            }
        }
    }
}

#[test]
fn erased_owner_with_a_kept_layer_uses_the_erased_group_forms() {
    let live = group(7, 9, vec![on(2, line(1.0)), on(2, line(3.0))]);
    // Without earlier member erasure: the uniform-negative form of the bare group.
    let source = drawing(vec![Item::Erased(on(2, Entity::Repeat(live.clone())))]);
    let bare = drawing(vec![Item::Erased(Entity::Repeat(live.clone()))]);
    assert!(!has_ambiguous_owner(&source));
    for version in [Version::Ac12, Version::Ac140] {
        let bytes = writes(&source, version).unwrap();
        assert_eq!(bytes, writes(&bare, version).unwrap());
        assert_eq!(acad_dwg::parse(&bytes).unwrap().items, bare.items);
    }
    // With earlier member erasure: the SAVE/END question's original form.
    let mut prior = live.clone();
    prior.entities[0] = erased(on(2, line(1.0)));
    let source = drawing(vec![Item::Erased(on(2, Entity::Repeat(prior.clone())))]);
    assert!(has_ambiguous_owner(&source));
    assert!(writes(&source, Version::Ac140)
        .unwrap_err()
        .contains("earlier member erasure lost"));
    let (converted, count) = original_member_erasure(&source);
    assert_eq!(count, 1);
    let all_erased = group(
        7,
        9,
        vec![erased(on(2, line(1.0))), erased(on(2, line(3.0)))],
    );
    assert_eq!(converted.items, [Item::Repeat(all_erased)]);
    for version in [Version::Ac12, Version::Ac140] {
        let bytes = writes(&converted, version).unwrap();
        assert_eq!(acad_dwg::parse(&bytes).unwrap().items, converted.items);
    }
    // A kept nested owner inside an ambiguous owner has its members erased too.
    let nested = group(
        7,
        9,
        vec![
            erased(on(2, line(1.0))),
            on(2, Entity::Repeat(group(5, 6, vec![on(2, line(3.0))]))),
        ],
    );
    let source = drawing(vec![Item::Erased(Entity::Repeat(nested))]);
    let (converted, _) = original_member_erasure(&source);
    let Item::Repeat(outer) = &converted.items[0] else {
        panic!("live markers")
    };
    let Entity::OnLayer { entity, .. } = &outer.entities[1] else {
        panic!("nested owner kept")
    };
    let Entity::Repeat(inner) = entity.as_ref() else {
        panic!("nested group")
    };
    assert!(inner.entities.iter().all(Entity::is_erased));
    assert!(writes(&converted, Version::Ac140).is_ok());
    // An owner layer no file keeps is neither asked about nor converted.
    let mut mismatch = prior;
    mismatch.entities[1] = on(3, line(3.0));
    let source = drawing(vec![Item::Erased(on(2, Entity::Repeat(mismatch)))]);
    assert!(!has_ambiguous_owner(&source));
    assert_eq!(original_member_erasure(&source), (source.clone(), 0));
    assert!(writes(&source, Version::Ac140)
        .unwrap_err()
        .contains("owner layer"));
}
