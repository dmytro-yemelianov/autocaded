use acad_model::{Entity, Item};
use acad_render::{flatten_selected_with_libraries, flatten_with_libraries, Libraries, Viewport};
use std::collections::BTreeSet;
fn libraries() -> Libraries {
    let mut libraries = Libraries::default();
    libraries
        .insert("TXT", b"*0,4,Vertical\n2,1,0,0\n*65,5,A\n024,2,030,1,0")
        .unwrap();
    libraries
        .insert("ALT", b"*0,4,Horizontal\n4,1,0,0\n*65,5,A\n040,2,010,1,0")
        .unwrap();
    libraries.insert("ES", b"*129,4,R\n3,2,024,0").unwrap();
    libraries
}
#[test]
fn erased_geometry_insert_and_load_members_have_no_effect_in_full_or_selected_rendering() {
    let mut actual = acad_dxf::parse(b"LOAD,1\r\nTXT\r\nREPEAT,3\r\nLOAD,2\r\nALT\r\nPOINT,2\r\n1000,1000\r\nTEXT,1\r\n0,0,1,0\r\nA\r\nENDREP,4\r\n2,1,3,0\r\nBLOCK,1\r\n0,0\r\nB\r\nLOAD,2\r\nALT\r\nPOINT,2\r\n999,999\r\nINSERT,2\r\n0,0,1,1,0\r\nB\r\nLOAD,1\r\nES\r\nSHAPE,1\r\n10,0,1,0,129\r\nENDBLK,1\r\nINSERT,1\r\n0,0,1,1,0\r\nB\r\nTEXT,1\r\n20,0,1,0\r\nA\r\nTEXT,5\r\n30,0,1,0\r\nA\r\n").unwrap();
    let mut expected = actual.clone();
    let Item::Repeat(expected_group) = &mut expected.items[1] else {
        panic!("group")
    };
    expected_group.entities.drain(..2);
    let Item::Block(expected_block) = &mut expected.items[2] else {
        panic!("block")
    };
    expected_block.entities.drain(..3);
    let Item::Repeat(group) = &mut actual.items[1] else {
        panic!("group")
    };
    for i in 0..2 {
        group.entities[i] = Entity::Erased(Box::new(group.entities[i].clone()));
    }
    let Item::Block(block) = &mut actual.items[2] else {
        panic!("block")
    };
    for i in 0..3 {
        block.entities[i] = Entity::Erased(Box::new(block.entities[i].clone()));
    }
    let before = actual.clone();
    let vp = Viewport::fit(&actual.header.limits, 800, 600);
    let wanted = flatten_with_libraries(&expected, &vp, &libraries());
    assert_eq!(wanted.primitives.len(), 5);
    assert!(wanted.diagnostics.is_empty());
    let rendered = flatten_with_libraries(&actual, &vp, &libraries());
    assert_eq!(rendered.primitives, wanted.primitives);
    assert!(
        rendered.diagnostics.is_empty(),
        "{:?}",
        rendered.diagnostics
    );
    for indexes in [
        BTreeSet::from([1]),
        BTreeSet::from([3]),
        BTreeSet::from([4]),
    ] {
        let selected = flatten_selected_with_libraries(&actual, &vp, &libraries(), &indexes);
        let desired = flatten_selected_with_libraries(&expected, &vp, &libraries(), &indexes);
        assert_eq!(selected.primitives, desired.primitives);
        assert!(selected.diagnostics.is_empty());
    }
    actual.header.off_layers.insert(2);
    assert_eq!(
        flatten_with_libraries(&actual, &vp, &libraries()).primitives,
        wanted.primitives
    );
    actual.header.off_layers.remove(&2);
    // An OFF owner still executes its definition's hidden LOADs, but must stop
    // at erased members: the later layer-5 TEXT keeps the unchanged font context.
    actual.header.off_layers.insert(1);
    expected.header.off_layers.insert(1);
    let hidden_wanted = flatten_with_libraries(&expected, &vp, &libraries());
    assert!(!hidden_wanted.primitives.is_empty());
    let hidden = flatten_with_libraries(&actual, &vp, &libraries());
    assert_eq!(hidden.primitives, hidden_wanted.primitives);
    assert_eq!(hidden.diagnostics, hidden_wanted.diagnostics);
    actual.header.off_layers.remove(&1);
    assert_eq!(actual, before, "rendering has no document mutation");
}
