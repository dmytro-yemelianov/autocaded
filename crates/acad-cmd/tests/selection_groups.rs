//! Rust editor numbering/group contracts; no original ID parity is asserted.
use acad_cmd::{selectable_items, Editor, Effect};
use acad_model::{Block, Entity, Item, Point, Repeat};

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn repeat() -> Repeat {
    Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![Entity::Line {
            start: point(1.0, 2.0),
            end: point(3.0, 2.0),
        }],
        columns: 3,
        rows: 2,
        column_spacing: -10.0,
        row_spacing: 5.0,
    }
}
fn fixture() -> Editor {
    let mut editor = Editor::default();
    editor.drawing_mut().items = vec![
        Item::Entity(Entity::Point {
            origin: point(40.0, 40.0),
        }),
        Item::Entity(Entity::OnLayer {
            layer: 2,
            entity: Box::new(Entity::Load {
                name: "FONT".into(),
            }),
        }),
        Item::Block(Block {
            name: "B".into(),
            base: point(0.0, 0.0),
            entities: vec![],
        }),
        Item::Repeat(repeat()),
        Item::Erased(Entity::Point {
            origin: point(90.0, 90.0),
        }),
        Item::Entity(Entity::Load {
            name: "OTHER".into(),
        }),
        Item::Entity(Entity::OnLayer {
            layer: 3,
            entity: Box::new(Entity::Repeat(repeat())),
        }),
        Item::Entity(Entity::Circle {
            center: point(50.0, 50.0),
            radius: 2.0,
        }),
    ];
    editor
}
fn submit(editor: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        editor.submit(input).unwrap();
    }
}

#[test]
fn traversal_list_and_picks_share_repeat_group_numbers() {
    let mut editor = fixture();
    assert_eq!(
        selectable_items(editor.drawing())
            .map(|item| (item.id, item.item_index))
            .collect::<Vec<_>>(),
        vec![(1, 0), (2, 3), (3, 6), (4, 7)]
    );
    assert_eq!(editor.pick_entity_at(point(-18.0, 7.0), 0.0), Some(2));
    assert_eq!(editor.pick_entity_at(point(52.0, 50.0), 0.0), Some(4));
    let before = editor.drawing().clone();
    editor.submit("LIST").unwrap();
    let Effect::Report(report) = editor.submit("ALL").unwrap() else {
        panic!("LIST report");
    };
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.status(), "1 POINT, 2 REPEAT, 3 REPEAT, 4 CIRCLE");
    assert!(report.contains("2: members=1 columns=3 rows=2 column spacing=-10.0000"));
    assert!(report.contains("3: layer=3 members=1 columns=3"));
    assert!(report.contains("4: layer=1 center=(50.0000,50.0000) radius=2.0000"));
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &["ERASE", "LAST"]);
    assert_eq!(selectable_items(editor.drawing()).count(), 3);
}

#[test]
fn move_copy_scale_erase_oops_and_undo_keep_whole_groups() {
    let mut editor = fixture();
    let original = editor.drawing().clone();
    submit(&mut editor, &["MOVE", "4,1", "", "2"]);
    assert_eq!(editor.pick_entity_at(point(-14.0, 8.0), 0.0), Some(2));
    let Item::Repeat(moved) = &editor.drawing().items[3] else {
        panic!("stored group");
    };
    assert_eq!(
        (
            moved.columns,
            moved.rows,
            moved.column_spacing,
            moved.row_spacing
        ),
        (3, 2, -10.0, 5.0)
    );
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &original);
    submit(&mut editor, &["COPY", "100,0", "", "2"]);
    assert!(matches!(
        editor.drawing().items.last(),
        Some(Item::Repeat(_))
    ));
    assert_eq!(editor.pick_entity_at(point(82.0, 7.0), 0.0), Some(5));
    submit(&mut editor, &["SCALE", "5", "0,0", "2"]);
    assert_eq!(editor.pick_entity_at(point(164.0, 14.0), 0.0), Some(5));
    let before_erase = editor.drawing().clone();
    submit(&mut editor, &["ERASE", "2,5"]);
    assert_eq!(selectable_items(editor.drawing()).count(), 3);
    assert!(matches!(
        &editor.drawing().items[3],
        Item::Erased(Entity::Repeat(_))
    ));
    submit(&mut editor, &["OOPS"]);
    assert_eq!(editor.drawing(), &before_erase);
    submit(&mut editor, &["UNDO"]);
    assert_eq!(selectable_items(editor.drawing()).count(), 3);
}

#[test]
fn block_wblock_and_array_include_selected_repeat_groups() {
    let mut editor = fixture();
    submit(&mut editor, &["WBLOCK", "group.dwg", "", "0,0"]);
    let Effect::SaveDrawing(_, drawing) = editor.submit("2").unwrap() else {
        panic!("selected WBLOCK");
    };
    assert_eq!(
        drawing
            .items
            .iter()
            .filter(|item| matches!(item, Item::Repeat(_)))
            .cloned()
            .collect::<Vec<_>>(),
        vec![Item::Repeat(repeat())]
    );
    assert_eq!(
        drawing
            .entities()
            .filter(|entity| matches!(entity, Entity::Load { .. } | Entity::OnLayer { .. }))
            .count(),
        2
    );
    submit(&mut editor, &["BLOCK", "GROUP", "0,0", "2"]);
    let block = editor.drawing().block("GROUP").unwrap();
    assert_eq!(block.entities, vec![Entity::Repeat(repeat())]);
    assert_eq!(selectable_items(editor.drawing()).count(), 3);
    submit(
        &mut editor,
        &["UNDO", "ARRAY", "2", "R", "1", "2", "0", "100"],
    );
    assert_eq!(selectable_items(editor.drawing()).count(), 5);
    assert_eq!(editor.pick_entity_at(point(82.0, 7.0), 0.0), Some(5));
}

#[test]
fn unsupported_repeat_operations_fail_without_mutation_or_undo() {
    for inputs in [
        vec!["ROTATE", "2", "0,0", "90"],
        vec!["CHANGE", "2", "0,0"],
        vec!["HATCH", "LINE", "1", "0", "2"],
        vec!["ENTITYAREA", "2"],
        vec!["BREAK", "2", "0,0", "1,0"],
    ] {
        let mut editor = fixture();
        let before = editor.drawing().clone();
        submit(&mut editor, &inputs[..inputs.len() - 1]);
        assert!(editor.submit(inputs.last().unwrap()).is_err(), "{inputs:?}");
        assert_eq!(editor.drawing(), &before);
        editor.cancel_command().unwrap();
        submit(&mut editor, &["UNDO"]);
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn hatch_after_repeat_uses_canonical_circle_id() {
    let mut editor = fixture();
    submit(&mut editor, &["HATCH", "LINE", "1", "0", "4"]);
    assert_eq!(selectable_items(editor.drawing()).count(), 5);
    assert!(editor.drawing().blocks().any(|block| block.name == "*X1"));
}

#[test]
fn detailed_list_bounds_large_groups_text_and_number_of_objects() {
    let mut editor = Editor::default();
    let mut large = repeat();
    large.rows = u16::MAX;
    large.columns = u16::MAX;
    editor.drawing_mut().items.push(Item::Repeat(large));
    editor.drawing_mut().items.push(Item::Entity(Entity::Text {
        origin: point(1.0, 2.0),
        height: 3.0,
        rotation_deg: 45.0,
        value: "abc\n\"".repeat(10_000),
    }));
    for n in 0..1001 {
        editor.drawing_mut().items.push(Item::Entity(Entity::Point {
            origin: point(f64::from(n), 0.0),
        }));
    }
    let before = editor.drawing().clone();
    editor.submit("LIST").unwrap();
    let Effect::Report(report) = editor.submit("ALL").unwrap() else {
        panic!("LIST");
    };
    assert!(report.contains("columns=65535 rows=65535"));
    assert!(report.contains("abc\\n\\\""));
    assert!(report.contains("LIST limit: 1000 of 1003 objects detailed; 3 omitted"));
    assert!(report.len() < 256_100);
    assert_eq!(
        editor.pick_entity_at(point(-18.0, 7.0), 0.0),
        None,
        "oversized group is skipped rather than partially scanned"
    );
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn deeply_nested_repeat_edit_bounds_do_not_expand_generated_cells() {
    let mut child = Entity::Point {
        origin: point(1.0, 2.0),
    };
    for _ in 0..64 {
        child = Entity::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![child],
            columns: 2,
            rows: 2,
            column_spacing: -3.0,
            row_spacing: 5.0,
        });
    }
    let mut editor = Editor::default();
    editor.drawing_mut().items.push(Item::Entity(child));
    submit(&mut editor, &["MOVE", "4,1", "", "1"]);
    assert_eq!(
        editor.drawing().header.extents,
        acad_model::Extents {
            xmin: -187.0,
            xmax: 5.0,
            ymin: 3.0,
            ymax: 323.0,
        }
    );
    submit(&mut editor, &["UNDO"]);
    assert_eq!(selectable_items(editor.drawing()).count(), 1);
}

fn layer_records(entity: &Entity, layers: &mut Vec<u8>) {
    match entity {
        Entity::OnLayer { layer, entity } => {
            if !matches!(entity.as_ref(), Entity::Repeat(_)) {
                layers.push(*layer);
            }
            if let Entity::Repeat(repeat) = entity.as_ref() {
                for entity in &repeat.entities {
                    layer_records(entity, layers);
                }
            }
        }
        Entity::Repeat(repeat) => {
            for entity in &repeat.entities {
                layer_records(entity, layers);
            }
        }
        _ => layers.push(1),
    }
}

#[test]
fn change_repeat_layer_updates_all_descendants_and_survives_both_dwg_revisions() {
    for representation in 0..3 {
        let mut group = repeat();
        group.entities.push(Entity::OnLayer {
            layer: 3,
            entity: Box::new(Entity::Point {
                origin: point(4.0, 5.0),
            }),
        });
        let mut editor = Editor::default();
        editor.drawing_mut().items = vec![match representation {
            0 => Item::Repeat(group),
            1 => Item::Entity(Entity::Repeat(group)),
            _ => Item::Entity(Entity::OnLayer {
                layer: 2,
                entity: Box::new(Entity::Repeat(group)),
            }),
        }];
        submit(&mut editor, &["CHANGE", "1", "L", "7"]);
        for version in [
            acad_dwg::header::Version::Ac12,
            acad_dwg::header::Version::Ac140,
        ] {
            if representation == 2 {
                let Item::Entity(entity) = &editor.drawing().items[0] else {
                    panic!("owner");
                };
                let mut layers = Vec::new();
                layer_records(entity, &mut layers);
                assert_eq!(layers, vec![7, 7], "owned group still changes in memory");
                assert!(acad_dwg::write_version(editor.drawing(), version)
                    .unwrap_err()
                    .to_string()
                    .contains("owner layer"));
                continue;
            }
            let reopened =
                acad_dwg::parse(&acad_dwg::write_version(editor.drawing(), version).unwrap())
                    .unwrap();
            let mut layers = Vec::new();
            for item in &reopened.items {
                match item {
                    Item::Entity(entity) => layer_records(entity, &mut layers),
                    Item::Repeat(repeat) => {
                        for entity in &repeat.entities {
                            layer_records(entity, &mut layers);
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(layers, vec![7, 7], "{representation} {version:?}");
        }
    }
}

#[test]
fn circular_array_repeat_uses_first_geometric_member_after_load() {
    let mut group = repeat();
    group.entities.insert(
        0,
        Entity::Load {
            name: "FONT".into(),
        },
    );
    let mut editor = Editor::default();
    editor.drawing_mut().items.push(Item::Repeat(group));
    submit(&mut editor, &["ARRAY", "1", "C", "0,0", "90", "2"]);
    assert_eq!(selectable_items(editor.drawing()).count(), 2);
    assert_eq!(editor.pick_entity_at(point(-1.0, 1.0), 1e-12), Some(2));
    assert_eq!(editor.status(), "Created circular array with 1 copies");
}

#[test]
fn selected_repeat_wblock_keeps_nested_block_closure_and_load_metadata() {
    let mut editor = Editor::default();
    let insert = |name: &str| Entity::Insert {
        origin: point(0.0, 0.0),
        x_scale: 1.0,
        y_scale: 1.0,
        rotation_deg: 0.0,
        name: name.into(),
    };
    editor.drawing_mut().items = vec![
        Item::Entity(Entity::Load {
            name: "FONT".into(),
        }),
        Item::Block(Block {
            name: "B".into(),
            base: point(0.0, 0.0),
            entities: vec![insert("C")],
        }),
        Item::Block(Block {
            name: "C".into(),
            base: point(0.0, 0.0),
            entities: vec![Entity::Line {
                start: point(0.0, 0.0),
                end: point(1.0, 0.0),
            }],
        }),
        Item::Block(Block {
            name: "UNUSED".into(),
            base: point(0.0, 0.0),
            entities: vec![],
        }),
        Item::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![insert("B")],
            columns: 2,
            rows: 1,
            column_spacing: 4.0,
            row_spacing: 0.0,
        }),
    ];
    submit(&mut editor, &["WBLOCK", "group.dwg", "", "0,0"]);
    let Effect::SaveDrawing(_, drawing) = editor.submit("1").unwrap() else {
        panic!("WBLOCK");
    };
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let reopened =
            acad_dwg::parse(&acad_dwg::write_version(&drawing, version).unwrap()).unwrap();
        assert_eq!(
            reopened
                .blocks()
                .map(|block| block.name.as_str())
                .collect::<Vec<_>>(),
            vec!["B", "C"]
        );
        assert!(reopened.entities().any(|entity| matches!(entity, Entity::OnLayer { entity, .. } if matches!(entity.as_ref(), Entity::Load { name } if name == "FONT"))));
        assert_eq!(selectable_items(&reopened).count(), 1);
    }
}

#[test]
fn recursive_repeat_layer_policy_changes_all_stored_leaf_records() {
    let mut group = repeat();
    group.entities.push(Entity::OnLayer {
        layer: 5,
        entity: Box::new(Entity::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![
                Entity::Load {
                    name: "FONT".into(),
                },
                Entity::Point {
                    origin: point(3.0, 4.0),
                },
            ],
            columns: 1,
            rows: 1,
            column_spacing: 0.0,
            row_spacing: 0.0,
        })),
    });
    let mut editor = Editor::default();
    editor.drawing_mut().items.push(Item::Repeat(group));
    submit(&mut editor, &["CHANGE", "1", "L", "7"]);
    let Item::Repeat(changed) = &editor.drawing().items[0] else {
        panic!("REPEAT");
    };
    let mut layers = Vec::new();
    for entity in &changed.entities {
        layer_records(entity, &mut layers);
    }
    assert_eq!(layers, vec![7, 7, 7]);
}

#[test]
fn circular_array_without_geometric_anchor_refuses_before_mutation_or_undo() {
    let mut group = repeat();
    group.entities = vec![Entity::Load {
        name: "FONT".into(),
    }];
    let mut editor = Editor::default();
    editor.drawing_mut().items.push(Item::Repeat(group));
    let before = editor.drawing().clone();
    submit(&mut editor, &["ARRAY", "1", "C", "0,0", "90"]);
    assert!(editor.submit("2").unwrap_err().contains("geometric anchor"));
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn erased_repeat_dwg_retains_owner_and_oops_restores_same_live_group() {
    let mut editor = Editor::default();
    let mut r = repeat();
    r.entities[0] = Entity::OnLayer {
        layer: 1,
        entity: Box::new(r.entities[0].clone()),
    };
    editor.drawing_mut().items.push(Item::Repeat(r));
    submit(&mut editor, &["ERASE", "1"]);
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let bytes = acad_dwg::write_version(editor.drawing(), version).unwrap();
        let reopened = acad_dwg::parse(&bytes).unwrap();
        assert_eq!(reopened.items, editor.drawing().items);
        assert_eq!(selectable_items(&reopened).count(), 0);
    }
    submit(&mut editor, &["OOPS"]);
    let reopened = acad_dwg::parse(&acad_dwg::write(editor.drawing()).unwrap()).unwrap();
    assert_eq!(selectable_items(&reopened).count(), 1);
}
