use acad_cmd::{selectable_items, Editor, Effect};
use acad_model::{Entity, Item};
fn source() -> acad_model::Drawing {
    let mut drawing = acad_dxf::parse(b"REPEAT,7\r\nPOINT,2\r\n100,200\r\nPOINT,3\r\n10,20\r\nLOAD,4\r\nALT\r\nENDREP,9\r\n2,1,5,0\r\nBLOCK,1\r\n0,0\r\nB\r\nPOINT,5\r\n0,0\r\nLOAD,6\r\nES\r\nENDBLK,1\r\nINSERT,1\r\n50,50,1,1,0\r\nB\r\n").unwrap();
    let Item::Repeat(r) = &mut drawing.items[0] else {
        panic!("group")
    };
    for i in [0, 2] {
        r.entities[i] = Entity::Erased(Box::new(r.entities[i].clone()));
    }
    let Item::Block(b) = &mut drawing.items[1] else {
        panic!("block")
    };
    for entity in &mut b.entities {
        *entity = Entity::Erased(Box::new(entity.clone()));
    }
    drawing
}
fn submit(editor: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        editor.submit(input).unwrap();
    }
}
#[test]
fn picks_windows_anchors_and_reports_use_live_members_while_ids_remain_owner_based() {
    // Positive markers plus negative members reopen identically in both revisions,
    // with the same owner IDs and live-only picking.
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let reopened =
            acad_dwg::parse(&acad_dwg::write_version(&source(), version).unwrap()).unwrap();
        assert_eq!(reopened.items, source().items);
        assert_eq!(reopened.entities().count(), 2, "live POINT and INSERT only");
        let reopened = Editor::new(reopened);
        assert_eq!(
            selectable_items(reopened.drawing())
                .map(|i| i.id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            reopened.pick_entity_at(acad_model::Point { x: 100.0, y: 200.0 }, 0.1),
            None
        );
        assert_eq!(
            reopened.pick_entity_at(acad_model::Point { x: 10.0, y: 20.0 }, 0.1),
            Some(1)
        );
    }
    let mut editor = Editor::new(source());
    let before = editor.drawing().clone();
    assert_eq!(
        selectable_items(editor.drawing())
            .map(|i| i.id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        editor.pick_entity_at(acad_model::Point { x: 100.0, y: 200.0 }, 0.1),
        None
    );
    assert_eq!(
        editor.pick_entity_at(acad_model::Point { x: 10.0, y: 20.0 }, 0.1),
        Some(1)
    );
    assert_eq!(
        editor.pick_entity_at(acad_model::Point { x: 50.0, y: 50.0 }, 0.1),
        None,
        "no invisible INSERT origin"
    );
    submit(&mut editor, &["MOVE", "1,2", "", "1"]);
    let moved = editor.drawing().clone();
    submit(&mut editor, &["ERASE", "W", "100,201"]);
    assert!(editor
        .submit("107,203")
        .unwrap_err()
        .contains("no visible objects"));
    assert_eq!(
        editor.drawing(),
        &moved,
        "a window around erased geometry selects no owner"
    );
    assert!(editor.collected_selection().is_empty());
    editor.cancel_command().unwrap();
    submit(&mut editor, &["UNDO"]);
    assert_eq!(
        editor.drawing(),
        &before,
        "window collection adds no undo snapshot"
    );
    submit(&mut editor, &["LIST"]);
    let Effect::Report(report) = editor.submit("ALL").unwrap() else {
        panic!("report")
    };
    // Object IDs go to the status line; the report is AutoCAD 1.4's layout.
    assert!(
        editor.status().starts_with("1 REPEAT, 2 INSERT"),
        "{}",
        editor.status()
    );
    assert!(report.contains("    erased members   2"), "{report}");
    assert!(!report.contains("100,200"));
    assert_eq!(editor.drawing(), &before);
}
#[test]
fn whole_owner_move_scale_layer_changes_preserve_member_status_and_undo() {
    let mut editor = Editor::new(source());
    let before = editor.drawing().clone();
    submit(
        &mut editor,
        &[
            "MOVE", "1,2", "", "1", "SCALE", "1", "0,0", "2", "CHANGE", "1", "L", "7",
        ],
    );
    let Item::Repeat(r) = &editor.drawing().items[0] else {
        panic!("group")
    };
    let Entity::Erased(hidden) = &r.entities[0] else {
        panic!("erased point")
    };
    assert!(
        matches!(hidden.as_ref(), Entity::OnLayer { layer: 7, entity } if matches!(entity.as_ref(), Entity::Point { origin } if *origin == acad_model::Point { x: 202.0, y: 404.0 }))
    );
    assert!(
        matches!(&r.entities[1], Entity::OnLayer { layer: 7, entity } if matches!(entity.as_ref(), Entity::Point { origin } if *origin == acad_model::Point { x: 22.0, y: 44.0 }))
    );
    assert!(r.entities[2].is_erased());
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        assert_eq!(
            acad_dwg::parse(&acad_dwg::write_version(editor.drawing(), version).unwrap())
                .unwrap()
                .items,
            editor.drawing().items
        );
    }
    submit(&mut editor, &["UNDO", "UNDO", "UNDO"]);
    assert_eq!(editor.drawing(), &before);
    submit(&mut editor, &["ARRAY", "1", "C", "0,0", "90", "2"]);
    let Item::Entity(Entity::Repeat(copy)) = &editor.drawing().items[3] else {
        panic!("copy")
    };
    assert!(copy.entities[0].is_erased());
    assert!(
        matches!(&copy.entities[1], Entity::OnLayer { entity, .. } if matches!(entity.as_ref(), Entity::Point { origin } if (origin.x + 20.0).abs() < 1e-12 && (origin.y - 10.0).abs() < 1e-12))
    );
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn erased_load_does_not_disqualify_startup_text_metrics_and_empty_live_owner_has_no_anchor() {
    let mut drawing = acad_dxf::parse(
        b"REPEAT,1\r\nLOAD,2\r\nMISSING\r\nPOINT,1\r\n0,0\r\nENDREP,1\r\n1,1,0,0\r\n",
    )
    .unwrap();
    let Item::Repeat(r) = &mut drawing.items[0] else {
        panic!("group")
    };
    r.entities[0] = Entity::Erased(Box::new(r.entities[0].clone()));
    let mut editor = Editor::new(drawing);
    submit(&mut editor, &["TEXT", "R", "1,2", "1", "0", "A"]);
    assert!(
        matches!(editor.drawing().items.last(), Some(Item::Entity(Entity::OnLayer { entity, .. })) if matches!(entity.as_ref(), Entity::Text { .. }))
    );
    submit(&mut editor, &["UNDO"]);
    let Item::Repeat(r) = &mut editor.drawing_mut().items[0] else {
        panic!("group")
    };
    r.entities[1] = Entity::Erased(Box::new(r.entities[1].clone()));
    let before = editor.drawing().clone();
    assert_eq!(
        selectable_items(&before).count(),
        1,
        "stored live owner keeps its ID"
    );
    assert_eq!(
        editor.pick_entity_at(acad_model::Point { x: 0.0, y: 0.0 }, 1.0),
        None
    );
    submit(&mut editor, &["ARRAY", "1", "C", "0,0", "90"]);
    assert!(
        editor.submit("2").is_err(),
        "no erased member supplies an ARRAY anchor"
    );
    assert_eq!(editor.drawing(), &before);
    editor.cancel_command().unwrap();
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn list_counts_erased_members_in_nested_groups() {
    let mut drawing = acad_dxf::parse(b"REPEAT,1\r\nPOINT,2\r\n1,1\r\nREPEAT,1\r\nPOINT,3\r\n2,2\r\nPOINT,4\r\n3,3\r\nPOINT,1\r\n4,4\r\nENDREP,1\r\n1,1,0,0\r\nPOINT,1\r\n5,5\r\nENDREP,1\r\n1,1,0,0\r\n").unwrap();
    let Item::Repeat(outer) = &mut drawing.items[0] else {
        panic!("outer")
    };
    outer.entities[0] = Entity::Erased(Box::new(outer.entities[0].clone()));
    let Entity::Repeat(inner) = &mut outer.entities[1] else {
        panic!("inner")
    };
    for i in [0, 1] {
        inner.entities[i] = Entity::Erased(Box::new(inner.entities[i].clone()));
    }
    let mut editor = Editor::new(drawing);
    submit(&mut editor, &["LIST"]);
    let Effect::Report(report) = editor.submit("ALL").unwrap() else {
        panic!("report")
    };
    assert!(report.contains("REPEAT start"), "{report}");
    assert!(report.contains("    erased members   3"), "{report}");
    assert!(
        !report.contains("2,2") && !report.contains("1,1 "),
        "{report}"
    );
}
