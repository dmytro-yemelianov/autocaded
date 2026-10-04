use acad_model::{Entity, Item, Point};
use acad_render::{flatten_entity, flatten_with_libraries, Libraries, Viewport};

fn group(columns: u16, rows: u16) -> acad_model::Repeat {
    let source = format!("REPEAT,1\r\nPOINT,1\r\n1,1\r\nENDREP,1\r\n{columns},{rows},1,1\r\n");
    let drawing = acad_dxf::parse(source.as_bytes()).unwrap();
    match drawing.items.into_iter().next().unwrap() {
        Item::Repeat(repeat) => repeat,
        _ => panic!("REPEAT"),
    }
}

#[test]
fn ordinary_frame_rejects_huge_flat_and_nested_owners_before_expansion() {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n2,2\r\n").unwrap();
    let point = drawing.items[0].clone();
    let mut nested = group(1_000, 1);
    nested.entities = vec![Entity::Repeat(group(1_000, 1))];
    drawing.items = vec![
        Item::Repeat(group(u16::MAX, u16::MAX)),
        Item::Repeat(nested),
        point,
    ];
    let before = drawing.clone();
    let vp = Viewport::fit(&drawing.header.limits, 800, 600);
    let output = flatten_with_libraries(&drawing, &vp, &Libraries::default());
    assert_eq!(output.primitives.len(), 2, "only the trailing POINT cross");
    assert_eq!(
        output
            .diagnostics
            .iter()
            .filter(|message| message.contains("render work budget"))
            .count(),
        2
    );
    assert_eq!(drawing, before);
    let Item::Repeat(repeat) = &drawing.items[1] else {
        unreachable!()
    };
    assert!(flatten_entity(&Entity::Repeat(repeat.clone()), &vp).is_empty());
}

#[test]
fn rejected_owner_keeps_ordered_font_load_for_the_following_text() {
    let mut drawing = acad_dxf::parse(b"LOAD,1\r\nALT\r\nTEXT,1\r\n2,2,1,0\r\nA\r\n").unwrap();
    let load = match drawing.items.remove(0) {
        Item::Entity(entity) => entity,
        _ => panic!("LOAD"),
    };
    let mut repeat = group(u16::MAX, u16::MAX);
    repeat.entities.insert(0, load);
    drawing.items.insert(0, Item::Repeat(repeat));
    let mut libraries = Libraries::default();
    libraries
        .insert("ALT", b"*0,4,ALT\n21,7,2,0\n*65,3,A\n1,021,0\n")
        .unwrap();
    let vp = Viewport::fit(&drawing.header.limits, 800, 600);
    let output = flatten_with_libraries(&drawing, &vp, &libraries);
    assert_eq!(output.primitives.len(), 1);
    assert_eq!(output.diagnostics.len(), 1);
    assert!(output.diagnostics[0].contains("render work budget"));
    drawing.items.remove(0);
    drawing
        .items
        .insert(0, Item::Entity(Entity::Load { name: "ALT".into() }));
    assert_eq!(
        output.primitives,
        flatten_with_libraries(&drawing, &vp, &libraries).primitives
    );
}

#[test]
fn excessive_stored_depth_stops_before_rendering_with_stale_resource_context() {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n2,2\r\n").unwrap();
    let mut load = Entity::Load { name: "ALT".into() };
    for _ in 0..300 {
        load = Entity::OnLayer {
            layer: 1,
            entity: Box::new(load),
        };
    }
    drawing.items.insert(0, Item::Entity(load));
    let vp = Viewport::fit(&drawing.header.limits, 800, 600);
    let output = flatten_with_libraries(&drawing, &vp, &Libraries::default());
    assert!(output.primitives.is_empty());
    assert!(output
        .diagnostics
        .iter()
        .any(|message| message.contains("LOAD context exceeds")));
    let geometry = Entity::OnLayer {
        layer: 1,
        entity: Box::new(Entity::Point {
            origin: Point { x: 2.0, y: 2.0 },
        }),
    };
    assert_eq!(flatten_entity(&geometry, &vp).len(), 2);
    assert!(flatten_entity(
        match &drawing.items[0] {
            Item::Entity(entity) => entity,
            _ => unreachable!(),
        },
        &vp
    )
    .is_empty());
}

#[test]
fn cyclic_reference_context_stops_the_frame_without_following_geometry() {
    let drawing = acad_dxf::parse(
        b"BLOCK,1\r\n0,0\r\nLOOP\r\nINSERT,1\r\n0,0,1,1,0\r\nLOOP\r\nENDBLK,1\r\nINSERT,1\r\n0,0,1,1,0\r\nLOOP\r\nPOINT,1\r\n2,2\r\n",
    )
    .unwrap();
    let vp = Viewport::fit(&drawing.header.limits, 800, 600);
    let output = flatten_with_libraries(&drawing, &vp, &Libraries::default());
    assert!(output.primitives.is_empty());
    assert!(output
        .diagnostics
        .iter()
        .any(|message| message.contains("block recursion limit reached")));
}

#[test]
fn zero_outer_lattice_cannot_bypass_nested_base_expansion_preflight() {
    for zero_rows in [true, false] {
        let mut drawing = acad_dxf::parse(b"POINT,1\r\n2,2\r\n").unwrap();
        let mut outer = group(1, 1);
        if zero_rows {
            outer.rows = 0;
        } else {
            outer.columns = 0;
        }
        outer.entities = vec![Entity::Repeat(group(u16::MAX, u16::MAX))];
        drawing.items.insert(0, Item::Repeat(outer.clone()));
        let vp = Viewport::fit(&drawing.header.limits, 800, 600);
        let output = flatten_with_libraries(&drawing, &vp, &Libraries::default());
        assert_eq!(output.primitives.len(), 2, "only the trailing POINT cross");
        assert!(output
            .diagnostics
            .iter()
            .any(|message| message.contains("render work budget")));
        assert!(flatten_entity(&Entity::Repeat(outer), &vp).is_empty());
    }
}
