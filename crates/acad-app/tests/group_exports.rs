use acad_cmd::{Editor, Effect};
use acad_model::{Block, Entity, Item, Point, Repeat};
#[test]
fn named_export_keeps_nested_dependencies_base_and_ordered_root_loads() {
    let load = |name: &str| Item::Entity(Entity::Load { name: name.into() });
    let mut drawing = Editor::default().drawing().clone();
    drawing.items = vec![
        load("FIRST"),
        Item::Block(Block {
            name: "LEAF".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![Entity::Point {
                origin: Point { x: 0.0, y: 0.0 },
            }],
        }),
        load("SECOND"),
        Item::Block(Block {
            name: "GROUP".into(),
            base: Point { x: 7.0, y: 9.0 },
            entities: vec![Entity::Repeat(Repeat {
                start_layer: 2,
                end_layer: 4,
                columns: 2,
                rows: 1,
                column_spacing: 3.0,
                row_spacing: 0.0,
                entities: vec![
                    Entity::Load {
                        name: "INNER".into(),
                    },
                    Entity::Insert {
                        name: "LEAF".into(),
                        origin: Point { x: 0.0, y: 0.0 },
                        x_scale: 1.0,
                        y_scale: 1.0,
                        rotation_deg: 0.0,
                    },
                ],
            })],
        }),
        load("AFTER"),
    ];
    let mut editor = Editor::new(drawing.clone());
    editor.submit("WBLOCK").unwrap();
    editor.submit("export").unwrap();
    let Effect::SaveDrawing(_, exported) = editor.submit("GROUP").unwrap() else {
        panic!("export")
    };
    assert_eq!(editor.drawing(), &drawing);
    assert_eq!(exported.header.base, Point { x: 7.0, y: 9.0 });
    assert!(exported.block("LEAF").is_some());
    assert!(exported.block("GROUP").is_none());
    assert_eq!(&exported.items[1..3], &[load("FIRST"), load("SECOND")]);
    assert_eq!(
        exported.items.last(),
        Some(&Item::Entity(
            drawing.block("GROUP").unwrap().entities[0].clone()
        ))
    );
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let bytes = acad_dwg::write_version(&exported, version).unwrap();
        let parsed = acad_dwg::parse(&bytes).unwrap();
        assert!(parsed.block("LEAF").is_some());
        assert_eq!(acad_dwg::write_version(&parsed, version).unwrap(), bytes);
    }
    let bytes = acad_dxf::try_write(&exported).unwrap();
    assert_eq!(
        acad_dxf::try_write(&acad_dxf::parse(&bytes).unwrap()).unwrap(),
        bytes
    );
}

fn nested_context_source() -> acad_model::Drawing {
    let mut drawing = acad_dxf::parse(b"BLOCK,1\r\n0,0\r\nRES\r\nREPEAT,1\r\nLOAD,2\r\nALT\r\nLOAD,2\r\nES\r\nENDREP,1\r\n65535,65535,1,1\r\nENDBLK,1\r\nREPEAT,1\r\nINSERT,2\r\n0,0,1,1,0\r\nRES\r\nENDREP,1\r\n65535,65535,1,1\r\nBLOCK,1\r\n0,0\r\nGROUP\r\nTEXT,1\r\n0,0,1,0\r\nA\r\nSHAPE,1\r\n4,0,1,0,129\r\nENDBLK,1\r\nINSERT,1\r\n0,0,1,1,0\r\nGROUP\r\n").unwrap();
    // Hidden geometry still executes its ordered library records.
    drawing.header.off_layers.insert(2);
    drawing
}
fn test_libraries() -> acad_render::Libraries {
    let mut libs = acad_render::Libraries::default();
    libs.insert("TXT", b"*0,4,Vertical\n2,1,0,0\n*65,5,A\n024,2,030,1,0")
        .unwrap();
    libs.insert("ALT", b"*0,4,Horizontal\n4,1,0,0\n*65,5,A\n040,2,010,1,0")
        .unwrap();
    libs.insert("ES", b"*129,4,R\n3,2,024,0").unwrap();
    libs
}
#[test]
fn named_and_selected_exports_project_hidden_nested_font_shape_context_once() {
    let source = nested_context_source();
    let vp = acad_render::Viewport::fit(&source.header.limits, 800, 600);
    // Compare the selected source owner; general rendering's large-owner guard
    // is a separate root contribution. No generated repeat cells are needed.
    let original = acad_render::flatten_selected_with_libraries(
        &source,
        &vp,
        &test_libraries(),
        &std::collections::BTreeSet::from([3]),
    );
    assert_eq!(original.primitives.len(), 2);
    for named in [true, false] {
        let mut editor = Editor::new(source.clone());
        editor.submit("WBLOCK").unwrap();
        editor.submit("export").unwrap();
        let effect = if named {
            editor.submit("GROUP").unwrap()
        } else {
            editor.submit("").unwrap();
            editor.submit("0,0").unwrap();
            editor.submit("2").unwrap()
        };
        let Effect::SaveDrawing(_, exported) = effect else {
            panic!("export")
        };
        assert_eq!(editor.drawing(), &source);
        let rendered = acad_render::flatten_with_libraries(&exported, &vp, &test_libraries());
        assert!(
            rendered.diagnostics.is_empty(),
            "{:?}",
            rendered.diagnostics
        );
        assert_eq!(rendered.primitives, original.primitives);
        assert!(
            exported.block("RES").is_none(),
            "preceding geometry/definitions are not exported"
        );
        assert_eq!(exported.items.iter().filter(|item| matches!(item, Item::Entity(Entity::OnLayer { entity, .. }) if matches!(entity.as_ref(), Entity::Load { .. }))).count(), 2);
        let mut encodable = *exported;
        encodable.header.off_layers.clear();
        for version in [
            acad_dwg::header::Version::Ac12,
            acad_dwg::header::Version::Ac140,
        ] {
            let reopened =
                acad_dwg::parse(&acad_dwg::write_version(&encodable, version).unwrap()).unwrap();
            assert_eq!(
                acad_render::flatten_with_libraries(&reopened, &vp, &test_libraries()).primitives,
                original.primitives
            );
        }
        let reopened = acad_dxf::parse(&acad_dxf::try_write(&encodable).unwrap()).unwrap();
        assert_eq!(
            acad_render::flatten_with_libraries(&reopened, &vp, &test_libraries()).primitives,
            original.primitives
        );
    }
}
#[test]
fn unknown_cyclic_deep_and_oversized_context_refuses_export_without_mutation() {
    for case in 0..5 {
        let mut drawing = acad_dxf::parse(
            b"POINT,1\r\n0,0\r\nBLOCK,1\r\n0,0\r\nGROUP\r\nPOINT,1\r\n1,1\r\nENDBLK,1\r\n",
        )
        .unwrap();
        let insert = |name: &str| Entity::Insert {
            name: name.into(),
            origin: Point { x: 0.0, y: 0.0 },
            x_scale: 1.0,
            y_scale: 1.0,
            rotation_deg: 0.0,
        };
        match case {
            0 => drawing.items[0] = Item::Entity(insert("MISSING")),
            1 => {
                drawing.items[0] = Item::Entity(insert("CYCLE"));
                drawing.items.push(Item::Block(Block {
                    name: "CYCLE".into(),
                    base: Point { x: 0.0, y: 0.0 },
                    entities: vec![insert("CYCLE")],
                }));
            }
            2 => {
                let mut entity = Entity::Load { name: "ALT".into() };
                for _ in 0..258 {
                    entity = Entity::Repeat(Repeat {
                        start_layer: 1,
                        end_layer: 1,
                        entities: vec![entity],
                        columns: 1,
                        rows: 1,
                        column_spacing: 0.0,
                        row_spacing: 0.0,
                    });
                }
                drawing.items[0] = Item::Entity(entity);
            }
            3 => {
                drawing.items[0] = Item::Entity(insert("CHAIN0"));
                for i in 0..17 {
                    drawing.items.push(Item::Block(Block {
                        name: format!("CHAIN{i}"),
                        base: Point { x: 0.0, y: 0.0 },
                        entities: vec![if i == 16 {
                            Entity::Load { name: "ALT".into() }
                        } else {
                            insert(&format!("CHAIN{}", i + 1))
                        }],
                    }));
                }
            }
            _ => {
                drawing.items[0] = Item::Repeat(Repeat {
                    start_layer: 1,
                    end_layer: 1,
                    entities: vec![Entity::Load { name: "ALT".into() }; 100_001],
                    columns: 1,
                    rows: 1,
                    column_spacing: 0.0,
                    row_spacing: 0.0,
                })
            }
        }
        let mut editor = Editor::new(drawing.clone());
        editor.submit("WBLOCK").unwrap();
        editor.submit("export").unwrap();
        assert!(editor.submit("GROUP").unwrap_err().contains("LOAD context"));
        assert_eq!(editor.drawing(), &drawing);
        assert_eq!(editor.prompt(), "WBLOCK: block name (* for entire drawing)");
        editor.cancel_command().unwrap();
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &drawing);
    }
}

#[test]
fn erased_insert_group_keeps_referenced_definitions_live_and_has_no_load_effects() {
    let mut drawing = acad_dxf::parse(b"BLOCK,1\r\n0,0\r\nRES\r\nLOAD,2\r\nES\r\nPOINT,3\r\n1,2\r\nENDBLK,1\r\nLOAD,1\r\nTXT\r\nREPEAT,4\r\nLOAD,5\r\nALT\r\nREPEAT,6\r\nINSERT,7\r\n0,0,1,1,0\r\nRES\r\nENDREP,8\r\n2,1,3,0\r\nENDREP,9\r\n2,1,5,0\r\nTEXT,1\r\n0,0,1,0\r\nA\r\n").unwrap();
    let Item::Repeat(group) = drawing.items[2].clone() else {
        panic!("group")
    };
    drawing.items[2] = Item::Erased(Entity::Repeat(group));
    let definitions = drawing.block("RES").unwrap().clone();
    let vp = acad_render::Viewport::fit(&drawing.header.limits, 800, 600);
    let mut absent = drawing.clone();
    absent.items.remove(2);
    let expected = acad_render::flatten_with_libraries(&absent, &vp, &test_libraries());
    assert_eq!(expected.primitives.len(), 1);
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let bytes = acad_dwg::write_version(&drawing, version).unwrap();
        let reopened = acad_dwg::parse(&bytes).unwrap();
        assert_eq!(reopened.items, drawing.items);
        assert_eq!(reopened.block("RES"), Some(&definitions));
        let rendered = acad_render::flatten_with_libraries(&reopened, &vp, &test_libraries());
        assert_eq!(rendered.primitives, expected.primitives);
        assert!(rendered.diagnostics.is_empty());
        assert_eq!(acad_dwg::write_version(&reopened, version).unwrap(), bytes);
    }
    let reopened = acad_dxf::parse(&acad_dxf::try_write(&drawing).unwrap()).unwrap();
    assert_eq!(reopened.items, absent.items);
    assert_eq!(reopened.block("RES"), Some(&definitions));
    assert_eq!(
        acad_render::flatten_with_libraries(&reopened, &vp, &test_libraries()).primitives,
        expected.primitives
    );
}
