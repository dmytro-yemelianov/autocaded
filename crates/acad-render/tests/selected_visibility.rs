use acad_model::{Entity, Item, Point, Repeat};
use acad_render::{flatten_selected_with_libraries, Libraries, Viewport};
use std::collections::BTreeSet;
#[test]
fn selected_owner_budget_rejects_huge_lattice_before_expansion_without_partial_output() {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,1\r\n").unwrap();
    drawing.items = vec![Item::Repeat(Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![Entity::Point {
            origin: Point { x: 1.0, y: 1.0 },
        }],
        columns: u16::MAX,
        rows: u16::MAX,
        column_spacing: 1.0,
        row_spacing: 1.0,
    })];
    let before = drawing.clone();
    let vp = Viewport::fit(&drawing.header.limits, 800, 600);
    let output =
        flatten_selected_with_libraries(&drawing, &vp, &Libraries::default(), &BTreeSet::from([0]));
    assert!(output.primitives.is_empty());
    assert!(output
        .diagnostics
        .iter()
        .any(|text| text.contains("highlight work budget")));
    assert_eq!(drawing, before);
}
#[test]
fn exceeded_unselected_library_context_fails_closed_instead_of_highlighting_with_stale_font() {
    let mut drawing = acad_dxf::parse(b"LOAD,1\r\nALT\r\nTEXT,1\r\n2,0,1,0\r\nA\r\n").unwrap();
    let Item::Entity(mut load) = drawing.items.remove(0) else {
        panic!("LOAD")
    };
    for _ in 0..300 {
        load = Entity::OnLayer {
            layer: 1,
            entity: Box::new(load),
        };
    }
    drawing.items.insert(0, Item::Entity(load));
    let vp = Viewport::fit(&drawing.header.limits, 800, 600);
    let output =
        flatten_selected_with_libraries(&drawing, &vp, &Libraries::default(), &BTreeSet::from([1]));
    assert!(output.primitives.is_empty());
    assert!(output
        .diagnostics
        .iter()
        .any(|text| text.contains("LOAD context exceeds")));
}
