use super::*;
use acad_model::{Entity, Item, Point, Repeat};
use acad_render::flatten_with_libraries;
fn libraries() -> Libraries {
    let mut libraries = Libraries::default();
    libraries
        .insert("TXT", b"*0,4,Vertical\n2,1,0,0\n*65,5,A\n024,2,030,1,0")
        .unwrap();
    libraries
        .insert("ALT", b"*0,4,Horizontal\n4,1,0,0\n*65,5,A\n040,2,010,1,0")
        .unwrap();
    libraries
}
fn fixture() -> acad_model::Drawing {
    acad_dxf::parse(b"BLOCK,1\r\n0,0\r\nFONT\r\nLOAD,2\r\nALT\r\nENDBLK,1\r\nINSERT,2\r\n0,0,1,1,0\r\nFONT\r\nTEXT,1\r\n2,0,1,0\r\nA\r\n").unwrap()
}
#[test]
fn selected_text_highlight_keeps_loads_from_unselected_hidden_insert_and_repeat() {
    let mut drawing = fixture();
    drawing.header.off_layers.insert(2);
    let vp = Viewport::fit(&drawing.header.limits, 800, 600);
    let before = drawing.clone();
    let expected =
        highlight_primitives(flatten_with_libraries(&drawing, &vp, &libraries()).primitives);
    assert_eq!(
        selected_highlight(&drawing, "2", &vp, &libraries()),
        expected
    );
    assert_eq!(drawing, before);
    let Item::Entity(insert) = drawing.items.remove(1) else {
        panic!("insert")
    };
    drawing.items.insert(
        1,
        Item::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![insert],
            columns: 2,
            rows: 2,
            column_spacing: 10.0,
            row_spacing: 10.0,
        }),
    );
    assert_eq!(
        selected_highlight(&drawing, "2", &vp, &libraries()),
        expected
    );
}
#[test]
fn hidden_selected_owner_and_mixed_repeat_have_only_visible_highlight_geometry() {
    let mut drawing = fixture();
    let line = |layer| Entity::OnLayer {
        layer,
        entity: Box::new(Entity::Line {
            start: Point { x: 1.0, y: 1.0 },
            end: Point { x: 2.0, y: 1.0 },
        }),
    };
    drawing.items = vec![Item::Repeat(Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![line(1), line(2)],
        columns: 2,
        rows: 1,
        column_spacing: 5.0,
        row_spacing: 0.0,
    })];
    drawing.header.off_layers.insert(2);
    let vp = Viewport::fit(&drawing.header.limits, 800, 600);
    assert_eq!(
        selected_highlight(&drawing, "1", &vp, &libraries()).len(),
        2
    );
    drawing.header.off_layers.insert(1);
    assert!(selected_highlight(&drawing, "1", &vp, &libraries()).is_empty());
}
#[test]
fn enormous_selected_repeat_is_rejected_before_generated_cell_expansion() {
    let mut drawing = fixture();
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
    let vp = Viewport::fit(&drawing.header.limits, 800, 600);
    assert!(selected_highlight(&drawing, "1", &vp, &libraries()).is_empty());
}
