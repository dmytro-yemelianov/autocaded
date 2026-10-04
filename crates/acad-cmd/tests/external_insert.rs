//! External drawing INSERT (docs/native-external-insert.md): the editor stays
//! filesystem-free; a decoded source drawing is staged at the name prompt and
//! committed atomically as one UNDO step.
use acad_cmd::Editor;
use acad_model::{Block, Drawing, Entity, Item, Point, Repeat};
use std::collections::{BTreeMap, BTreeSet};

const NAME_PROMPT: &str = "INSERT: block or file name";

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn line(x: f64) -> Entity {
    Entity::Line {
        start: p(x, 0.0),
        end: p(x + 1.0, 1.0),
    }
}
fn on(layer: u8, entity: Entity) -> Entity {
    Entity::OnLayer {
        layer,
        entity: Box::new(entity),
    }
}
fn insert(name: &str, x: f64) -> Entity {
    Entity::Insert {
        origin: p(x, 0.0),
        x_scale: 1.0,
        y_scale: 1.0,
        rotation_deg: 0.0,
        name: name.into(),
    }
}
fn block(name: &str, entities: Vec<Entity>) -> Item {
    Item::Block(Block {
        name: name.into(),
        base: p(0.0, 0.0),
        entities,
    })
}
fn source(base: Point, items: Vec<Item>) -> Drawing {
    let mut drawing = Editor::default().drawing().clone();
    drawing.header.base = base;
    drawing.header.layers = BTreeMap::from([(0, 0), (1, 15), (7, 3), (9, 4)]);
    drawing.header.off_layers = BTreeSet::from([7]);
    drawing.items = items;
    drawing
}
/// Host with one undoable LINE, so one UNDO after any refusal must empty it.
fn host() -> Editor {
    let mut editor = Editor::default();
    for input in ["LINE", "0,0", "1,1", ""] {
        editor.submit(input).unwrap();
    }
    editor
}
fn blocks(editor: &Editor) -> Vec<String> {
    editor.drawing().blocks().map(|b| b.name.clone()).collect()
}

#[test]
fn file_requests_only_arise_at_the_name_prompt_for_names_that_are_not_blocks() {
    let mut editor = host();
    assert_eq!(editor.insert_file_request("PART"), None);
    editor.submit("INSERT").unwrap();
    assert_eq!(editor.prompt(), NAME_PROMPT);
    assert_eq!(editor.insert_file_request("PART").as_deref(), Some("PART"));
    assert_eq!(
        editor.insert_file_request(" *B:part.dwg ").as_deref(),
        Some("B:part.dwg")
    );
    assert_eq!(editor.insert_file_request(""), None);
    assert_eq!(editor.insert_file_request("*"), None);
    editor
        .drawing_mut()
        .items
        .push(block("PART", vec![line(0.0)]));
    assert_eq!(editor.insert_file_request("part"), None);
    assert_eq!(editor.insert_file_request("*Part"), None);
    // Without a Session nothing is read: the old unknown-block refusal remains.
    let before = editor.drawing().clone();
    assert!(editor
        .submit("MISSING")
        .unwrap_err()
        .contains("unknown block"));
    assert_eq!(editor.prompt(), NAME_PROMPT);
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn block_form_uses_stored_base_negative_scales_and_one_undo() {
    let mut editor = host();
    let before = editor.drawing().clone();
    let repeat = Repeat {
        start_layer: 9,
        end_layer: 9,
        entities: vec![line(3.0)],
        columns: 2,
        rows: 1,
        column_spacing: 1.0,
        row_spacing: 0.0,
    };
    let src = source(
        p(2.0, 3.0),
        vec![
            block(
                "NUT",
                vec![Entity::Circle {
                    center: p(0.0, 0.0),
                    radius: 1.0,
                }],
            ),
            Item::Entity(on(7, line(5.0))),
            Item::Entity(insert("NUT", 1.0)),
            Item::Repeat(repeat.clone()),
            Item::Erased(on(1, line(8.0))),
        ],
    );
    editor.submit("INSERT").unwrap();
    editor
        .submit_insert_drawing("C:/parts/bolt.dwg", src)
        .unwrap();
    assert_eq!(editor.prompt(), "INSERT: insertion point");
    assert_eq!(
        editor.drawing(),
        &before,
        "nothing changes before the last prompt"
    );
    for input in ["10,10", "-2", "", "30"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(blocks(&editor), ["NUT", "BOLT"]);
    let bolt = editor.drawing().block("BOLT").unwrap();
    assert_eq!(bolt.base, p(2.0, 3.0));
    assert_eq!(
        bolt.entities,
        vec![
            on(7, line(5.0)),
            insert("NUT", 1.0),
            Entity::Repeat(repeat),
            on(1, Entity::Erased(Box::new(line(8.0)))),
        ]
    );
    let Some(Item::Entity(Entity::OnLayer { layer: 1, entity })) = editor.drawing().items.last()
    else {
        panic!("INSERT on the current layer")
    };
    assert_eq!(
        **entity,
        Entity::Insert {
            origin: p(10.0, 10.0),
            x_scale: -2.0,
            y_scale: -2.0,
            rotation_deg: 30.0,
            name: "BOLT".into()
        }
    );
    // Layers 7 and 9 were new: file colours, and 7's OFF state, propagate.
    let header = &editor.drawing().header;
    assert_eq!(header.layers.get(&7), Some(&3));
    assert_eq!(header.layers.get(&9), Some(&4));
    assert!(header.off_layers.contains(&7));
    assert_eq!(header.current_layer, 1);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn existing_host_layers_keep_their_colour_and_visibility() {
    let mut editor = host();
    editor.drawing_mut().header.layers.insert(7, 1);
    let src = source(p(0.0, 0.0), vec![Item::Entity(on(7, line(0.0)))]);
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("PART", src).unwrap();
    for input in ["0,0", "", "", ""] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().header.layers.get(&7), Some(&1));
    assert!(!editor.drawing().header.off_layers.contains(&7));
}

#[test]
fn star_form_translates_by_base_and_imports_only_reachable_definitions() {
    let mut editor = host();
    let before = editor.drawing().clone();
    let src = source(
        p(1.0, 1.0),
        vec![
            block("INNER", vec![line(0.0)]),
            block("NUT", vec![insert("INNER", 0.0)]),
            block("UNUSED", vec![line(9.0)]),
            block("GHOST", vec![line(7.0)]),
            Item::Entity(insert("NUT", 2.0)),
            Item::Erased(Entity::Insert {
                origin: p(0.0, 0.0),
                x_scale: 1.0,
                y_scale: 1.0,
                rotation_deg: 0.0,
                name: "GHOST".into(),
            }),
            Item::Entity(Entity::Point {
                origin: p(3.0, 4.0),
            }),
        ],
    );
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("*bolt", src).unwrap();
    assert_eq!(editor.drawing(), &before);
    editor.submit("5,5").unwrap();
    assert_eq!(editor.prompt(), "Command");
    // Erased records keep their dependencies; no definition named BOLT.
    assert_eq!(blocks(&editor), ["INNER", "NUT", "GHOST"]);
    let items = &editor.drawing().items;
    let tail = &items[items.len() - 3..];
    let mut moved = insert("NUT", 6.0);
    if let Entity::Insert { origin, .. } = &mut moved {
        origin.y = 4.0;
    }
    assert_eq!(tail[0], Item::Entity(moved));
    assert!(
        matches!(&tail[1], Item::Erased(Entity::Insert { origin, .. }) if *origin == p(4.0, 4.0))
    );
    assert_eq!(
        tail[2],
        Item::Entity(Entity::Point {
            origin: p(7.0, 8.0)
        })
    );
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn identical_nested_definitions_are_reused_not_duplicated() {
    let mut editor = host();
    editor
        .drawing_mut()
        .items
        .push(block("NUT", vec![line(0.0)]));
    let src = source(
        p(0.0, 0.0),
        vec![
            block("NUT", vec![line(0.0)]),
            Item::Entity(insert("NUT", 0.0)),
        ],
    );
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("BOLT", src).unwrap();
    for input in ["0,0", "", "", ""] {
        editor.submit(input).unwrap();
    }
    assert_eq!(blocks(&editor), ["NUT", "BOLT"]);
}

#[test]
fn conflicts_cycles_and_invalid_sources_are_refused_without_mutation() {
    let nut = |x| block("NUT", vec![line(x)]);
    let cases: Vec<(&str, Vec<Item>, &str)> = vec![
        (
            "B:HOSTED.DWG",
            vec![Item::Entity(line(0.0))],
            "already exists",
        ),
        (
            "BOLT",
            vec![nut(5.0), Item::Entity(insert("NUT", 0.0))],
            "different content",
        ),
        (
            "BOLT",
            vec![
                block("nut", vec![line(0.0)]),
                Item::Entity(insert("nut", 0.0)),
            ],
            "differs from existing block NUT in name case;",
        ),
        (
            "BOLT",
            vec![
                block("A", vec![line(0.0)]),
                block("a", vec![line(1.0)]),
                Item::Entity(insert("A", 0.0)),
                Item::Entity(insert("a", 0.0)),
            ],
            "more than once",
        ),
        (
            "BOLT",
            vec![Item::Entity(insert("NOPE", 0.0))],
            "undefined block",
        ),
        (
            "BOLT",
            vec![
                block("A", vec![insert("B", 0.0)]),
                block("B", vec![insert("A", 0.0)]),
                Item::Entity(insert("A", 0.0)),
            ],
            "cyclic",
        ),
        (
            "BOLT",
            vec![
                block("bolt", vec![line(0.0)]),
                Item::Entity(insert("bolt", 0.0)),
            ],
            "conflicts with",
        ),
        ("BOLT", vec![block("NUT", vec![line(0.0)])], "no entities"),
        ("C:/dir/.dwg", vec![Item::Entity(line(0.0))], "block name"),
        ("bölt.dxf", vec![Item::Entity(line(0.0))], "block name"),
        (
            "BOLT",
            vec![Item::Entity(Entity::Circle {
                center: p(f64::NAN, 0.0),
                radius: 1.0,
            })],
            "finite",
        ),
    ];
    for star in [false, true] {
        for (name, items, expected) in &cases {
            // The star form creates no definition named after the file.
            if star && matches!(*expected, "already exists" | "conflicts with") {
                continue;
            }
            let mut editor = host();
            editor.drawing_mut().items.push(nut(0.0));
            editor
                .drawing_mut()
                .items
                .push(block("HOSTED", vec![line(0.0)]));
            let before = editor.drawing().clone();
            editor.submit("INSERT").unwrap();
            let input = if star {
                format!("*{name}")
            } else {
                name.to_string()
            };
            let error = editor
                .submit_insert_drawing(&input, source(p(0.0, 0.0), items.clone()))
                .unwrap_err();
            assert!(
                error.contains(expected),
                "{input}: {error:?} should mention {expected:?}"
            );
            assert_eq!(editor.prompt(), NAME_PROMPT, "{input}: retry prompt");
            assert_eq!(editor.drawing(), &before, "{input}");
            editor.cancel_command().unwrap();
            editor.submit("UNDO").unwrap();
            assert!(
                editor.drawing().entities().next().is_none(),
                "{input}: a refusal must not push UNDO history"
            );
        }
    }
}

#[test]
fn star_form_of_a_hosted_name_conflict_is_still_a_definition_check() {
    // `*B:HOSTED.DWG` creates no HOSTED definition, so it is accepted.
    let mut editor = host();
    editor
        .drawing_mut()
        .items
        .push(block("HOSTED", vec![line(0.0)]));
    editor.submit("INSERT").unwrap();
    editor
        .submit_insert_drawing(
            "*B:HOSTED.DWG",
            source(p(0.0, 0.0), vec![Item::Entity(line(4.0))]),
        )
        .unwrap();
    editor.submit("0,0").unwrap();
    assert_eq!(blocks(&editor), ["HOSTED"]);
    assert_eq!(
        editor.drawing().items.last(),
        Some(&Item::Entity(line(4.0)))
    );
}

#[test]
fn cancel_and_retry_at_every_prompt_discard_the_staged_import() {
    let src = source(
        p(0.0, 0.0),
        vec![
            block("NUT", vec![line(0.0)]),
            Item::Entity(insert("NUT", 0.0)),
        ],
    );
    for prompts in [0, 1, 2, 3] {
        let mut editor = host();
        let before = editor.drawing().clone();
        editor.submit("INSERT").unwrap();
        editor.submit_insert_drawing("BOLT", src.clone()).unwrap();
        for input in ["1,1", "2", "3"].iter().take(prompts) {
            editor.submit(input).unwrap();
        }
        // An invalid answer keeps the prompt and the staged import.
        let prompt = editor.prompt().to_owned();
        assert!(editor.submit("nonsense").is_err());
        assert_eq!(editor.prompt(), prompt);
        assert_eq!(editor.drawing(), &before);
        if prompts % 2 == 0 {
            editor.cancel_command().unwrap();
        } else {
            editor
                .apply_menu_control(acad_cmd::MenuControl::Cancel)
                .unwrap();
        }
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing(), &before, "cancel after {prompts} answers");
        // A later INSERT of the same name has nothing staged left over.
        editor.submit("INSERT").unwrap();
        assert!(editor.submit("BOLT").unwrap_err().contains("unknown block"));
    }
    // Zero scales are refused at their prompts, then the command completes.
    let mut editor = host();
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("BOLT", src).unwrap();
    editor.submit("1,1").unwrap();
    assert!(editor.submit("0").is_err());
    editor.submit("1").unwrap();
    assert!(editor.submit("0").is_err());
    editor.submit("").unwrap();
    editor.submit("").unwrap();
    assert_eq!(blocks(&editor), ["NUT", "BOLT"]);
}

#[test]
fn box_scale_applies_to_an_external_block() {
    let mut editor = host();
    editor.submit("INSERT").unwrap();
    editor
        .submit_insert_drawing("BOLT", source(p(0.0, 0.0), vec![Item::Entity(line(0.0))]))
        .unwrap();
    for input in ["3,3", "5,6", "0"] {
        editor.submit(input).unwrap();
    }
    let Some(Item::Entity(Entity::OnLayer { entity, .. })) = editor.drawing().items.last() else {
        panic!("insert")
    };
    assert!(
        matches!(**entity, Entity::Insert { x_scale, y_scale, .. } if x_scale == 2.0 && y_scale == 3.0)
    );
}

#[test]
fn star_form_live_root_load_becomes_the_shape_library() {
    let mut editor = host();
    editor.register_shape_library("ES", [("A".to_owned(), 129)]);
    editor.submit("SHAPE").unwrap();
    assert!(editor.submit("A").is_err());
    editor.cancel_command().unwrap();
    let src = source(
        p(0.0, 0.0),
        vec![Item::Entity(Entity::Load { name: "ES".into() })],
    );
    // Block form: a LOAD inside a definition does not change the active library.
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("FONTS", src.clone()).unwrap();
    for input in ["0,0", "", "", ""] {
        editor.submit(input).unwrap();
    }
    editor.submit("SHAPE").unwrap();
    assert!(editor.submit("A").is_err());
    editor.cancel_command().unwrap();
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("*FONTS2", src).unwrap();
    editor.submit("0,0").unwrap();
    editor.submit("SHAPE").unwrap();
    editor.submit("A").unwrap();
    editor.cancel_command().unwrap();
    editor.submit("UNDO").unwrap();
    editor.submit("SHAPE").unwrap();
    assert!(editor.submit("A").is_err());
}

fn points(count: usize) -> Vec<Item> {
    (0..count)
        .map(|i| {
            Item::Entity(Entity::Point {
                origin: p(i as f64, 0.0),
            })
        })
        .collect()
}

#[test]
fn host_plus_import_must_fit_the_whole_drawing_record_budget() {
    for star in [false, true] {
        let mut editor = host();
        editor.drawing_mut().items.extend(points(40_000));
        let before = editor.drawing().clone();
        editor.submit("INSERT").unwrap();
        let input = if star { "*BIG" } else { "BIG" };
        let error = editor
            .submit_insert_drawing(input, source(p(0.0, 0.0), points(30_000)))
            .unwrap_err();
        assert!(error.contains("65535"), "{error}");
        assert_eq!(editor.prompt(), NAME_PROMPT);
        assert_eq!(editor.drawing(), &before);
        // The same import fits once the host is small enough.
        editor
            .submit_insert_drawing(input, source(p(0.0, 0.0), points(25_000)))
            .unwrap();
    }
}

#[test]
fn block_form_reports_omitted_erased_groups() {
    let mut editor = host();
    let group = Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![line(0.0)],
        columns: 2,
        rows: 1,
        column_spacing: 1.0,
        row_spacing: 0.0,
    };
    let src = source(
        p(0.0, 0.0),
        vec![Item::Entity(line(3.0)), Item::Erased(Entity::Repeat(group))],
    );
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("ERBARE", src.clone()).unwrap();
    for input in ["0,0", "", "", ""] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().block("ERBARE").unwrap().entities.len(), 1);
    assert_eq!(
        editor.status(),
        "Inserted drawing ERBARE; 1 erased REPEAT group(s) omitted, use *ERBARE to keep them"
    );
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("*ERBARE2", src).unwrap();
    editor.submit("0,0").unwrap();
    assert!(matches!(
        editor.drawing().items.last(),
        Some(Item::Erased(Entity::Repeat(_)))
    ));
}

#[test]
fn committed_imports_report_only_reachable_live_load_names() {
    let src = source(
        p(0.0, 0.0),
        vec![
            block("FONTS", vec![Entity::Load { name: "ES".into() }]),
            block("UNUSED", vec![Entity::Load { name: "PC".into() }]),
            Item::Entity(insert("FONTS", 0.0)),
            Item::Entity(on(
                1,
                Entity::Load {
                    name: "B:ROMAN".into(),
                },
            )),
            Item::Erased(Entity::Load {
                name: "ITALIC".into(),
            }),
        ],
    );
    let mut editor = host();
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("LIBS", src.clone()).unwrap();
    editor.cancel_command().unwrap();
    assert_eq!(editor.take_committed_import_loads(), None);
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("LIBS", src.clone()).unwrap();
    for input in ["0,0", "", "", ""] {
        editor.submit(input).unwrap();
    }
    assert_eq!(
        editor.take_committed_import_loads(),
        Some(vec!["B:ROMAN".to_owned(), "ES".to_owned()])
    );
    assert_eq!(editor.take_committed_import_loads(), None);
    editor.submit("INSERT").unwrap();
    editor.submit_insert_drawing("*LIBS2", src).unwrap();
    editor.submit("0,0").unwrap();
    // FONTS is reused from the host; its LOAD is still reachable.
    assert_eq!(
        editor.take_committed_import_loads(),
        Some(vec!["B:ROMAN".to_owned(), "ES".to_owned()])
    );
}
