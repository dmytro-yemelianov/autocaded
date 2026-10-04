//! Empty REPEAT groups in DXF (docs/native-group-persistence.md, "R6 empty
//! REPEAT groups"). AutoCAD 1.4's task 5 writes a live group's REPEAT and
//! ENDREP records even when it has no live member, and its task 6 reads the
//! pair back; `crates/acad-oracle/tests/empty_repeat.rs` asserts both. The
//! native reader and writer do the same.
use acad_model::{Entity, Item, Point, Repeat};

/// The original's task 5 DXF of its own empty-pair drawing
/// (acad-dwg/tests/fixtures/README.md).
const ORIGINAL: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/empty-repeat-ended.dxf");
const PAIR: &str = "REPEAT,1\r\nENDREP,1\r\n2,1,5.000000,0.000000\r\n";

fn empty(columns: u16, rows: u16, column_spacing: f64, row_spacing: f64) -> Repeat {
    Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: Vec::new(),
        columns,
        rows,
        column_spacing,
        row_spacing,
    }
}

fn line(y: f64) -> Entity {
    Entity::OnLayer {
        layer: 1,
        entity: Box::new(Entity::Line {
            start: Point { x: 1.0, y },
            end: Point { x: 2.0, y },
        }),
    }
}

fn entity_section(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).into_owned();
    let text = &text[..text.find('\u{1a}').unwrap()];
    // After the eight LAYERC rows.
    let layerc = text.find("LAYERC,1\r\n").unwrap();
    text[layerc..].splitn(10, "\r\n").nth(9).unwrap().to_owned()
}

#[test]
fn the_originals_dxf_with_an_empty_pair_opens_and_rewrites_identically() {
    let drawing = acad_dxf::parse(ORIGINAL).unwrap();
    assert_eq!(drawing.items, [Item::Repeat(empty(2, 1, 5.0, 0.0))]);
    assert_eq!(entity_section(ORIGINAL), PAIR);
    let written = acad_dxf::try_write(&drawing).unwrap();
    // The native writer reproduces the original's file up to its end
    // marker (the original pads the rest of its last sector).
    let end = ORIGINAL.iter().position(|&b| b == 0x1a).unwrap();
    assert_eq!(written, ORIGINAL[..=end]);
    assert_eq!(acad_dxf::parse(&written).unwrap(), drawing);
}

#[test]
fn live_groups_keep_their_markers_without_live_members() {
    let mut drawing = acad_dxf::parse(ORIGINAL).unwrap();
    // As the original writes for its all-erased group: the pair.
    let mut all_erased = empty(2, 1, 5.0, 0.0);
    all_erased.entities = vec![
        Entity::Erased(Box::new(line(1.0))),
        Entity::Erased(Box::new(line(3.0))),
    ];
    drawing.items = vec![Item::Repeat(all_erased)];
    assert_eq!(
        entity_section(&acad_dxf::try_write(&drawing).unwrap()),
        PAIR
    );
    // Nested: the outer pair holds the inner pair.
    let mut outer = empty(1, 2, 5.0, 5.0);
    outer.entities = vec![Entity::Repeat(empty(2, 1, 5.0, 0.0))];
    drawing.items = vec![Item::Repeat(outer.clone())];
    let written = acad_dxf::try_write(&drawing).unwrap();
    assert_eq!(
        entity_section(&written),
        format!("REPEAT,1\r\n{PAIR}ENDREP,1\r\n1,2,5.000000,5.000000\r\n")
    );
    assert_eq!(acad_dxf::parse(&written).unwrap().items, drawing.items);
    // A group in a block, with its own marker layers.
    let mut layered = empty(3, 2, 1.5, -2.0);
    layered.start_layer = 3;
    layered.end_layer = 4;
    drawing.items = vec![
        Item::Block(acad_model::Block {
            name: "B".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![Entity::Repeat(layered)],
        }),
        Item::Repeat(outer),
    ];
    let written = acad_dxf::try_write(&drawing).unwrap();
    assert_eq!(acad_dxf::parse(&written).unwrap().items, drawing.items);
    // A whole erased owner, even an empty one, is still omitted.
    drawing.items = vec![Item::Erased(Entity::Repeat(empty(2, 1, 5.0, 0.0)))];
    assert_eq!(entity_section(&acad_dxf::try_write(&drawing).unwrap()), "");
}

#[test]
fn malformed_empty_pairs_are_still_refused() {
    let header_end = {
        let text = String::from_utf8_lossy(ORIGINAL).into_owned();
        let at = text.find("REPEAT,1").unwrap();
        text[..at].to_owned()
    };
    for body in [
        "REPEAT,1\r\nENDREP,1\r\n0,1,5,0\r\n",
        "REPEAT,1\r\nENDREP,1\r\n2,0,5,0\r\n",
        "REPEAT,1\r\nENDREP,1\r\n2.5,1,5,0\r\n",
        "REPEAT,1\r\nENDREP,1\r\n2,1\r\n",
        "REPEAT,1\r\n",
        "ENDREP,1\r\n2,1,5,0\r\n",
        "REPEAT,1\r\nBLOCK,1\r\n0,0\r\nB\r\nENDREP,1\r\n2,1,5,0\r\nENDBLK,1\r\n",
        "REPEAT,300\r\nENDREP,1\r\n2,1,5,0\r\n",
    ] {
        let text = format!("{header_end}{body}\u{1a}");
        assert!(acad_dxf::parse(text.as_bytes()).is_err(), "{body:?}");
    }
    // The bare valid pair parses.
    let text = format!("{header_end}REPEAT,7\r\nENDREP,8\r\n2,1,5,0\r\n\u{1a}");
    let mut want = empty(2, 1, 5.0, 0.0);
    want.start_layer = 7;
    want.end_layer = 8;
    assert_eq!(
        acad_dxf::parse(text.as_bytes()).unwrap().items,
        [Item::Repeat(want)]
    );
}
