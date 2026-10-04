//! Font resolution must fail closed instead of using a partially walked state.
use acad_model::{Block, Entity, Item, Point};
use acad_render::{text_font_at, Libraries};
fn insert(name: &str) -> Entity {
    Entity::Insert {
        name: name.into(),
        origin: Point { x: 0.0, y: 0.0 },
        x_scale: 1.0,
        y_scale: 1.0,
        rotation_deg: 0.0,
    }
}
#[test]
fn font_context_stops_on_cyclic_insert_and_stored_depth_exhaustion() {
    let mut d = acad_dxf::parse(b"POINT,1\r\n1,2\r\n").unwrap();
    d.items = vec![
        Item::Block(Block {
            name: "LOOP".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![
                Entity::Load {
                    name: "MISSING".into(),
                },
                insert("LOOP"),
            ],
        }),
        Item::Entity(insert("LOOP")),
    ];
    assert!(text_font_at(&d, d.items.len(), &Libraries::default())
        .unwrap_err()
        .contains("block recursion limit reached"));
    let mut deep = Entity::Load {
        name: "MISSING".into(),
    };
    for _ in 0..258 {
        deep = Entity::OnLayer {
            layer: 1,
            entity: Box::new(deep),
        };
    }
    d.items = vec![Item::Entity(deep)];
    assert!(text_font_at(&d, 1, &Libraries::default()).is_err());
}
#[test]
fn missing_font_name_is_retained_and_erased_or_block_definitions_do_not_switch_it() {
    let mut d = acad_dxf::parse(b"POINT,1\r\n1,2\r\n").unwrap();
    d.items = vec![
        Item::Erased(Entity::Load {
            name: "ERASED".into(),
        }),
        Item::Block(Block {
            name: "UNUSED".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![Entity::Load {
                name: "UNUSED".into(),
            }],
        }),
        Item::Entity(Entity::Load {
            name: "MISSING".into(),
        }),
    ];
    assert_eq!(text_font_at(&d, 2, &Libraries::default()).unwrap(), "TXT");
    assert_eq!(
        text_font_at(&d, 3, &Libraries::default()).unwrap(),
        "MISSING"
    );
    assert!(text_font_at(&d, 4, &Libraries::default()).is_err());
}
