//! Native star INSERT scale/rotation (docs/native-external-insert.md): the
//! `S` extension, member placement, representability refusals, erased-member
//! promotion and one UNDO. The original never asks for star placement; the
//! in-tree evidence is in acad-oracle/tests/insert_change.rs.
use acad_cmd::Editor;
use acad_model::{Block, Entity, Item, Point, Repeat};

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn on(entity: Entity) -> Entity {
    Entity::OnLayer {
        layer: 1,
        entity: Box::new(entity),
    }
}
fn bare(mut e: &Entity) -> &Entity {
    while let Entity::OnLayer { entity, .. } = e {
        e = entity;
    }
    e
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn near_point(a: Point, x: f64, y: f64) {
    near(a.x, x);
    near(a.y, y);
}
fn submit(e: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        e.submit(input)
            .unwrap_or_else(|error| panic!("{input}: {error}"));
    }
}
fn insert(name: &str, origin: Point, x: f64, y: f64, rotation: f64) -> Entity {
    Entity::Insert {
        origin,
        x_scale: x,
        y_scale: y,
        rotation_deg: rotation,
        name: name.into(),
    }
}

/// Block P (base 1,1) with a line, circle, arc, text, point, nested INSERT,
/// an erased line and a REPEAT lattice.
fn editor(members: Vec<Entity>) -> Editor {
    let mut e = Editor::default();
    e.drawing_mut().items = vec![
        Item::Block(Block {
            name: "Q".into(),
            base: p(0.0, 0.0),
            entities: vec![on(Entity::Line {
                start: p(0.0, 0.0),
                end: p(1.0, 0.0),
            })],
        }),
        Item::Block(Block {
            name: "P".into(),
            base: p(1.0, 1.0),
            entities: members,
        }),
    ];
    e
}
fn full() -> Vec<Entity> {
    vec![
        on(Entity::Line {
            start: p(1.0, 1.0),
            end: p(2.0, 1.0),
        }),
        on(Entity::Circle {
            center: p(2.0, 1.0),
            radius: 0.5,
        }),
        on(Entity::Arc {
            center: p(1.0, 2.0),
            radius: 1.0,
            start_deg: 0.0,
            end_deg: 90.0,
        }),
        on(Entity::Text {
            origin: p(1.0, 3.0),
            height: 0.5,
            rotation_deg: 10.0,
            value: "AB".into(),
        }),
        on(Entity::Point {
            origin: p(3.0, 1.0),
        }),
        on(insert("Q", p(2.0, 2.0), 1.0, 2.0, 0.0)),
        on(Entity::Erased(Box::new(Entity::Line {
            start: p(1.0, 1.0),
            end: p(1.0, 2.0),
        }))),
        Entity::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![on(Entity::Point {
                origin: p(1.0, 1.0),
            })],
            columns: 1,
            rows: 1,
            column_spacing: 1.0,
            row_spacing: 1.0,
        }),
    ]
}

fn root(e: &Editor, index: usize) -> &Entity {
    match &e.drawing().items[index] {
        Item::Entity(entity) | Item::Erased(entity) => bare(entity),
        other => panic!("{other:?}"),
    }
}

#[test]
fn identity_s_answers_equal_the_evidenced_translation() {
    let mut plain = editor(full());
    submit(&mut plain, &["INSERT", "*P", "5,5"]);
    let mut scaled = editor(full());
    submit(&mut scaled, &["INSERT", "*P", "s", "", "", "", "5,5"]);
    assert_eq!(plain.drawing(), scaled.drawing());
}

#[test]
fn uniform_scale_and_quarter_turn_place_every_member_kind() {
    let mut e = editor(full());
    let before = e.drawing().clone();
    submit(&mut e, &["INSERT", "*P", "S", "2", "", "90"]);
    assert!(
        e.prompt().starts_with("INSERT: insertion point"),
        "{}",
        e.prompt()
    );
    assert_eq!(e.drawing(), &before, "staged only");
    submit(&mut e, &["5,5"]);
    let Entity::Line { start, end } = root(&e, 2) else {
        panic!()
    };
    near_point(*start, 5.0, 5.0);
    near_point(*end, 5.0, 7.0);
    let Entity::Circle { center, radius } = root(&e, 3) else {
        panic!()
    };
    near_point(*center, 5.0, 7.0);
    near(*radius, 1.0);
    let Entity::Arc {
        center,
        radius,
        start_deg,
        end_deg,
    } = root(&e, 4)
    else {
        panic!()
    };
    near_point(*center, 3.0, 5.0);
    near(*radius, 2.0);
    near(*start_deg, 90.0);
    near(*end_deg, 180.0);
    let Entity::Text {
        origin,
        height,
        rotation_deg,
        ..
    } = root(&e, 5)
    else {
        panic!()
    };
    near_point(*origin, 1.0, 5.0);
    near(*height, 1.0);
    near(*rotation_deg, 100.0);
    let Entity::Insert {
        origin,
        x_scale,
        y_scale,
        rotation_deg,
        ..
    } = root(&e, 7)
    else {
        panic!()
    };
    near_point(*origin, 3.0, 7.0);
    assert_eq!((*x_scale, *y_scale, *rotation_deg), (2.0, 4.0, 90.0));
    // The erased member is promoted to a root erased record, never revived.
    assert!(matches!(e.drawing().items[8], Item::Erased(_)));
    let Entity::Line { start, end } = root(&e, 8) else {
        panic!()
    };
    near_point(*start, 5.0, 5.0);
    near_point(*end, 3.0, 5.0);
    // A single-cell REPEAT turns freely; lattices need axis-aligned placement
    // (`repeat_lattices_need_axis_aligned_placement`).
    let Entity::Repeat(repeat) = root(&e, 9) else {
        panic!()
    };
    let Entity::Point { origin } = bare(&repeat.entities[0]) else {
        panic!()
    };
    near_point(*origin, 5.0, 5.0);
    submit(&mut e, &["UNDO"]);
    assert_eq!(e.drawing(), &before);
}

#[test]
fn repeat_lattices_need_axis_aligned_placement() {
    let lattice = || {
        vec![Entity::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![on(Entity::Line {
                start: p(1.0, 1.0),
                end: p(2.0, 1.0),
            })],
            columns: 2,
            rows: 3,
            column_spacing: 1.0,
            row_spacing: 0.5,
        })]
    };
    let mut e = editor(lattice());
    submit(&mut e, &["INSERT", "*P", "S", "2", "3"]);
    let before = e.drawing().clone();
    assert!(e.submit("30").unwrap_err().contains("REPEAT lattice"));
    assert!(e.prompt().contains("rotation"));
    assert_eq!(e.drawing(), &before);
    submit(&mut e, &["180", "0,0"]);
    // Block members promote as entity-form REPEAT roots (root_item).
    let Entity::Repeat(repeat) = root(&e, 2) else {
        panic!()
    };
    assert_eq!((repeat.column_spacing, repeat.row_spacing), (-2.0, -1.5));
    let Entity::Line { start, end } = bare(&repeat.entities[0]) else {
        panic!()
    };
    assert_eq!((*start, *end), (p(0.0, 0.0), p(-2.0, 0.0)));
}

#[test]
fn non_uniform_scale_is_refused_only_for_unrepresentable_members() {
    // Lines alone take any scale.
    let mut lines = editor(vec![on(Entity::Line {
        start: p(1.0, 1.0),
        end: p(2.0, 2.0),
    })]);
    submit(&mut lines, &["INSERT", "*P", "S", "2", "3", "", "0,0"]);
    let Entity::Line { end, .. } = root(&lines, 2) else {
        panic!()
    };
    near_point(*end, 2.0, 3.0);
    for (member, scale) in [
        (
            on(Entity::Circle {
                center: p(1.0, 1.0),
                radius: 1.0,
            }),
            ["2", "3"],
        ),
        (
            on(Entity::Text {
                origin: p(1.0, 1.0),
                height: 1.0,
                rotation_deg: 0.0,
                value: "A".into(),
            }),
            ["-1", "1"],
        ),
        (on(insert("Q", p(1.0, 1.0), 1.0, 1.0, 45.0)), ["2", "1"]),
    ] {
        let mut e = editor(vec![member]);
        let before = e.drawing().clone();
        submit(&mut e, &["INSERT", "*P", "S", scale[0], scale[1]]);
        let error = e.submit("").unwrap_err();
        assert!(error.contains("insert as a block instead"), "{error}");
        assert!(e.prompt().contains("rotation"));
        e.cancel_command().unwrap();
        assert_eq!(e.drawing(), &before);
        let _ = e.submit("UNDO");
        assert_eq!(e.drawing(), &before, "a refusal leaves no undo step");
    }
}

#[test]
fn mirroring_reverses_arcs_and_composes_nested_inserts() {
    let mut e = editor(vec![
        on(Entity::Arc {
            center: p(1.0, 1.0),
            radius: 1.0,
            start_deg: 0.0,
            end_deg: 90.0,
        }),
        on(insert("Q", p(1.0, 1.0), 2.0, 1.0, 0.0)),
        on(insert("Q", p(1.0, 1.0), 1.0, 1.0, 90.0)),
    ]);
    submit(&mut e, &["INSERT", "*P", "S", "-1", "1", "", "0,0"]);
    let Entity::Arc {
        start_deg, end_deg, ..
    } = root(&e, 2)
    else {
        panic!()
    };
    near(*start_deg, 90.0);
    near(*end_deg, 180.0);
    let Entity::Insert {
        x_scale,
        y_scale,
        rotation_deg,
        ..
    } = root(&e, 3)
    else {
        panic!()
    };
    // diag(-1,1) * diag(2,1) = R(0) * diag(-2, 1): the sign stays on X.
    near(*x_scale, -2.0);
    near(*y_scale, 1.0);
    near(*rotation_deg, 0.0);
    let Entity::Insert {
        x_scale,
        y_scale,
        rotation_deg,
        ..
    } = root(&e, 4)
    else {
        panic!()
    };
    // diag(-1,1) * R(90) = R(90) * diag(1,-1).
    near(*x_scale, 1.0);
    near(*y_scale, -1.0);
    near(*rotation_deg, 90.0);
}

#[test]
fn s_is_only_a_star_keyword_and_scales_validate() {
    let mut e = editor(full());
    submit(&mut e, &["INSERT", "P"]);
    assert!(
        e.submit("S").is_err(),
        "plain INSERT keeps the point prompt"
    );
    e.cancel_command().unwrap();
    submit(&mut e, &["INSERT", "*P", "S"]);
    assert!(e.submit("0").is_err());
    assert!(e.submit("x").is_err());
    submit(&mut e, &["2"]);
    assert!(e.submit("0").is_err());
    assert!(e.prompt().contains("Y scale"));
}

#[test]
fn text_and_shape_angles_wrap_into_one_turn() {
    let mut e = editor(vec![
        on(Entity::Text {
            origin: p(1.0, 1.0),
            height: 1.0,
            rotation_deg: 15.0,
            value: "A".into(),
        }),
        on(Entity::Shape {
            origin: p(1.0, 1.0),
            height: 1.0,
            rotation_deg: 350.0,
            number: 1,
        }),
    ]);
    submit(&mut e, &["INSERT", "*P", "S", "1", "1", "360", "0,0"]);
    let Entity::Text { rotation_deg, .. } = root(&e, 2) else {
        panic!()
    };
    near(*rotation_deg, 15.0);
    let Entity::Shape { rotation_deg, .. } = root(&e, 3) else {
        panic!()
    };
    near(*rotation_deg, 350.0);
    submit(&mut e, &["INSERT", "*P", "S", "-1", "", "30", "0,0"]);
    let Entity::Shape { rotation_deg, .. } = root(&e, 5) else {
        panic!()
    };
    near(*rotation_deg, 200.0);
}
