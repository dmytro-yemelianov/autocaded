//! Constructed native layout/history/atomicity contracts; no original A/C/R parity.
use acad_cmd::Editor;
use acad_model::{Entity, Item, Point};
fn inputs(e: &mut Editor, values: &[&str]) {
    for value in values {
        e.submit(value).unwrap();
    }
}
fn bare(mut e: &Entity) -> &Entity {
    while let Entity::OnLayer { entity, .. } = e {
        e = entity;
    }
    e
}
fn text(e: &Editor, index: usize) -> (Point, f64, f64, &str) {
    let Item::Entity(e) = &e.drawing().items[index] else {
        panic!()
    };
    let Entity::Text {
        origin,
        height,
        rotation_deg,
        value,
    } = bare(e)
    else {
        panic!()
    };
    (*origin, *height, *rotation_deg, value)
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} vs {b}");
}
#[test]
fn centered_right_and_aligned_ink_at_rotated_baselines() {
    let mut e = Editor::default();
    inputs(&mut e, &["TEXT", "C", "10,20", "2", "90", "AB "]);
    let (p, h, a, v) = text(&e, 0);
    near(p.x, 10.0);
    near(p.y, 20.0 - 36.0 / 21.0);
    assert_eq!((h, a, v), (2.0, 90.0, "AB "));
    inputs(&mut e, &["TEXT", "R", "10,20", "2", "30", "A"]);
    let (p, _, _, _) = text(&e, 1);
    let offset = 2.0 * 16.0 / 21.0;
    near(p.x, 10.0 - offset * 30f64.to_radians().cos());
    near(p.y, 20.0 - offset * 0.5);
    inputs(&mut e, &["TEXT", "A", "1,2", "@3,4", "AB"]);
    let (p, h, a, _) = text(&e, 2);
    assert_eq!(p, Point { x: 1.0, y: 2.0 });
    near(h, 5.0 * 21.0 / 36.0);
    near(a, 4f64.atan2(3.0).to_degrees());
}
#[test]
fn height_and_angle_points_retry_finite_distinct_and_preserve_whitespace() {
    let mut e = Editor::default();
    inputs(&mut e, &["TEXT", "R", "1,2", "@2,3"]);
    assert!(e.accepts_mouse_point());
    assert!(e.submit("@0,0").is_err());
    inputs(&mut e, &["@3,4"]);
    assert!(e.submit("@0,0").is_err());
    assert!(e.submit("@1e308,1e308").is_ok());
    e.cancel_command().unwrap();
    assert!(e.drawing().items.is_empty());
    inputs(&mut e, &["TEXT", "1,2", "@2,3", "@3,4", "@0,5", " A "]);
    let (p, h, a, v) = text(&e, 0);
    assert_eq!(p, Point { x: 1.0, y: 2.0 });
    assert_eq!((h, a, v), (5.0, 90.0, " A "));
    inputs(&mut e, &["TEXT", "A", "1,1"]);
    assert!(e.submit("1,1").is_err());
    assert!(e.submit("@1e309,0").is_err());
    e.cancel_command().unwrap();
    inputs(&mut e, &["TEXT", "C", "0,0", "1", "0"]);
    assert!(e.submit("   ").is_err());
    assert!(e.submit("`").is_err());
    assert!(e.submit("é").is_err());
    inputs(&mut e, &["A"]);
}
#[test]
fn physical_repeat_preserves_logical_anchor_height_and_undo_history() {
    let mut e = Editor::default();
    e.submit_return("TEXT").unwrap();
    inputs(&mut e, &["C", "10,20", "2", "90", "AB"]);
    let original = e.drawing().clone();
    e.submit("").unwrap();
    assert_eq!(e.prompt(), "Command");
    assert_eq!(e.drawing(), &original);
    e.submit_return("").unwrap();
    assert_eq!(e.prompt(), "TEXT: value");
    inputs(&mut e, &["A"]);
    let (p, h, a, _) = text(&e, 1);
    near(p.x, 13.0);
    near(p.y, 20.0 - 16.0 / 21.0);
    assert_eq!((h, a), (2.0, 90.0));
    inputs(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &original);
    e.submit_return(" ").unwrap();
    inputs(&mut e, &["AB"]);
    let (p, _, _, _) = text(&e, 1);
    near(p.x, 13.0);
    e.submit_return("").unwrap();
    e.cancel_command().unwrap();
    assert_eq!(e.drawing().items.len(), 2);
    inputs(&mut e, &["ERASE", "2"]);
    assert!(
        e.submit_return("").is_err(),
        "erased source cannot continue"
    );
    let mut reopened = Editor::new(original);
    reopened.submit_return("TEXT").unwrap();
    reopened.cancel_command().unwrap();
    assert!(
        reopened.submit_return("").is_err(),
        "reopen has no session TEXT history"
    );
}
#[test]
fn aligned_repeat_preserves_effective_height_without_refitting_new_span() {
    let mut e = Editor::default();
    e.submit_return("T").unwrap();
    inputs(&mut e, &["A", "1,2", "@3,4", "AB"]);
    let (_, height, angle, _) = text(&e, 0);
    e.submit_return("").unwrap();
    inputs(&mut e, &["A"]);
    let (p, h, a, _) = text(&e, 1);
    assert_eq!((h, a), (height, angle));
    near(p.x, 1.0 + 1.5 * height * 0.8);
    near(p.y, 2.0 - 1.5 * height * 0.6);
}
#[test]
fn change_text_stages_origin_height_angle_value_cancel_retry_and_one_undo() {
    let mut e = Editor::default();
    inputs(&mut e, &["TEXT", "1,2", "3", "0", "OLD"]);
    let original = e.drawing().clone();
    inputs(&mut e, &["CHANGE", "1", "@2,3"]);
    assert!(e.prompt().contains("height"), "{}", e.prompt());
    assert_eq!(e.drawing(), &original);
    assert!(e.submit("bad").is_err());
    assert!(e.submit("-1").is_err());
    assert_eq!(e.drawing(), &original);
    inputs(&mut e, &["", "@0,4"]);
    assert_eq!(e.drawing(), &original);
    assert!(e.submit("é").is_err());
    assert_eq!(e.drawing(), &original);
    e.cancel_command().unwrap();
    assert_eq!(e.drawing(), &original);
    // Original: a new value erases the changed record and appends the text.
    inputs(&mut e, &["CHANGE", "1", "3,5", "", "90", " NEW "]);
    let Item::Erased(erased) = &e.drawing().items[0] else {
        panic!("changed TEXT record is kept erased")
    };
    assert!(
        matches!(bare(erased), Entity::Text { value, rotation_deg, .. } if value == "OLD" && *rotation_deg == 90.0)
    );
    let (p, h, a, v) = text(&e, 1);
    assert_eq!((p, h, a, v), (Point { x: 3.0, y: 5.0 }, 3.0, 90.0, " NEW "));
    inputs(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &original);
    inputs(&mut e, &["CHANGE", "1", "3,5", "2", "", ""]);
    assert_eq!(text(&e, 0), (Point { x: 3.0, y: 5.0 }, 2.0, 0.0, "OLD"));
    assert_eq!(e.drawing().items.len(), 1, "unchanged value stays in place");
    inputs(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &original);
    inputs(
        &mut e,
        &[
            "CHANGE", "1", "1,2", "", "", "", "CHANGE", "1", "", "", "", "", "CHANGE", "1", "L",
            "1", "UNDO",
        ],
    );
    assert!(
        e.drawing().items.is_empty(),
        "no-op property/layer CHANGE adds no undo"
    );
}
#[test]
fn mixed_text_and_insert_stage_atomically_in_reverse_order_with_one_undo() {
    let mut e = Editor::default();
    inputs(
        &mut e,
        &[
            "TEXT", "1,2", "3", "0", "OLD", "LINE", "0,0", "1,1", "", "BLOCK", "B", "0,0", "2",
            "INSERT", "B", "4,4", "1", "1", "0",
        ],
    );
    let original = e.drawing().clone();
    inputs(&mut e, &["CHANGE", "ALL", "8,8"]);
    assert!(
        e.prompt().starts_with("CHANGE: new angle"),
        "{}",
        e.prompt()
    );
    assert!(e.submit("bad").is_err());
    inputs(&mut e, &["45"]);
    assert!(e.prompt().contains("height"), "{}", e.prompt());
    assert_eq!(e.drawing(), &original);
    e.cancel_command().unwrap();
    assert_eq!(e.drawing(), &original);
    inputs(&mut e, &["CHANGE", "ALL", "8,8", "45", "2", "", "NEW"]);
    let undo_depth = e.drawing().items.len();
    assert_eq!(undo_depth, original.items.len() + 1);
    assert!(matches!(e.drawing().items[0], Item::Erased(_)));
    assert_eq!(
        text(&e, original.items.len()),
        (Point { x: 8.0, y: 8.0 }, 2.0, 0.0, "NEW")
    );
    let Item::Entity(insert) = e.drawing().items.iter().rev().nth(1).unwrap() else {
        panic!()
    };
    assert!(
        matches!(bare(insert), Entity::Insert { origin, rotation_deg, .. } if *origin == Point { x: 8.0, y: 8.0 } && *rotation_deg == 45.0)
    );
    inputs(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &original);
    inputs(&mut e, &["CHANGE", "LAST", "8,8"]);
    assert_eq!(e.drawing(), &original);
    assert!(e.submit("bad").is_err());
    e.cancel_command().unwrap();
    assert_eq!(e.drawing(), &original);
    inputs(&mut e, &["CHANGE", "LAST", "8,8", "@0,1"]);
    assert_ne!(e.drawing(), &original);
    inputs(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &original);
    inputs(&mut e, &["CHANGE", "1", "L", "3"]);
    assert!(matches!(
        &e.drawing().items[0],
        Item::Entity(Entity::OnLayer { layer: 3, .. })
    ));
    inputs(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &original);
}

#[test]
fn right_repeat_and_multi_ordinary_change_preserve_atomic_history() {
    let mut e = Editor::default();
    e.submit_return("TEXT").unwrap();
    inputs(&mut e, &["R", "10,20", "2", "30", "AB"]);
    e.submit_return("").unwrap();
    inputs(&mut e, &["A"]);
    let (p, h, a, _) = text(&e, 1);
    near(
        p.x,
        10.0 + 1.5 - 2.0 * 16.0 / 21.0 * 30f64.to_radians().cos(),
    );
    near(p.y, 20.0 - 3.0 * 30f64.to_radians().cos() - 16.0 / 21.0);
    assert_eq!((h, a), (2.0, 30.0));
    inputs(&mut e, &["TEXT", "A", "0,0", "1,0"]);
    assert!(e.submit("I").is_err());
    inputs(&mut e, &["AB"]);
    inputs(&mut e, &["CHANGE", "1,2", "0,0"]);
    assert!(e.prompt().contains("height"), "two TEXTs now stage in turn");
    e.cancel_command().unwrap();
    let mut ordinary = Editor::default();
    inputs(
        &mut ordinary,
        &[
            "LINE", "0,0", "1,1", "", "LINE", "3,3", "4,4", "", "BLOCK", "B", "0,0", "LAST",
            "INSERT", "B", "5,5", "1", "1", "0",
        ],
    );
    let before = ordinary.drawing().clone();
    inputs(&mut ordinary, &["CHANGE", "ALL", "8,8"]);
    assert_eq!(ordinary.drawing(), &before);
    ordinary.cancel_command().unwrap();
    assert_eq!(ordinary.drawing(), &before);
    inputs(&mut ordinary, &["CHANGE", "ALL", "8,8", ""]);
    assert_ne!(ordinary.drawing(), &before);
    inputs(&mut ordinary, &["UNDO"]);
    assert_eq!(ordinary.drawing(), &before);
}

#[test]
fn standalone_metrics_branching_self_insert_is_bounded_retryable_and_neutral() {
    use acad_model::Block;
    let insert = || Entity::Insert {
        name: "SELF".into(),
        origin: Point { x: 0.0, y: 0.0 },
        x_scale: 1.0,
        y_scale: 1.0,
        rotation_deg: 0.0,
    };
    let mut e = Editor::default();
    // Legal compact stored graph; the former sibling-vector algorithm would
    // enqueue ~100,000*4096 references before its visit budget stopped it.
    e.drawing_mut().items = vec![
        Item::Block(Block {
            name: "SELF".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: (0..4096).map(|_| insert()).collect(),
        }),
        Item::Entity(insert()),
    ];
    let original = e.drawing().clone();
    inputs(&mut e, &["TEXT", "C", "1,2", "1", "0"]);
    for value in ["AB", "BA"] {
        assert!(e.submit(value).unwrap_err().contains("cyclic INSERT"));
        assert_eq!(e.prompt(), "TEXT: value");
        assert_eq!(e.drawing(), &original);
    }
    e.cancel_command().unwrap();
    assert_eq!(e.prompt(), "Command");
    assert_eq!(e.drawing(), &original);
    inputs(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &original, "failed metrics/cancel add no undo");
}

#[test]
fn standalone_metrics_reject_unknown_deep_and_excessive_context_without_provider() {
    use acad_model::Block;
    let insert = |name: String| Entity::Insert {
        name,
        origin: Point { x: 0.0, y: 0.0 },
        x_scale: 1.0,
        y_scale: 1.0,
        rotation_deg: 0.0,
    };
    let point = Entity::Point {
        origin: Point { x: 0.0, y: 0.0 },
    };
    let mut cases = Vec::new();
    cases.push((
        vec![Item::Entity(insert("MISSING".into()))],
        "unknown block",
    ));
    let mut chain: Vec<Item> = (0..17)
        .map(|i| {
            Item::Block(Block {
                name: format!("B{i}"),
                base: Point { x: 0.0, y: 0.0 },
                entities: if i == 16 {
                    vec![point.clone()]
                } else {
                    vec![insert(format!("B{}", i + 1))]
                },
            })
        })
        .collect();
    chain.push(Item::Entity(insert("B0".into())));
    cases.push((chain, "block recursion limit reached"));
    let mut deep = point.clone();
    for _ in 0..257 {
        deep = Entity::OnLayer {
            layer: 1,
            entity: Box::new(deep),
        };
    }
    cases.push((vec![Item::Entity(deep)], "stored traversal budget"));
    cases.push((
        vec![
            Item::Block(Block {
                name: "WIDE".into(),
                base: Point { x: 0.0, y: 0.0 },
                entities: vec![point; 100_000],
            }),
            Item::Entity(insert("WIDE".into())),
        ],
        "stored traversal budget",
    ));
    for (items, error) in cases {
        let mut e = Editor::default();
        e.drawing_mut().items = items;
        let original = e.drawing().clone();
        inputs(&mut e, &["TEXT", "R", "1,2", "1", "0"]);
        assert!(e.submit("A").unwrap_err().contains(error));
        assert_eq!(e.drawing(), &original);
        e.cancel_command().unwrap();
        assert_eq!(e.drawing(), &original);
    }
}
