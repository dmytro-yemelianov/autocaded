use acad_cmd::{Editor, Effect};

fn submit(editor: &mut Editor, inputs: &[&str]) {
    for input in inputs {
        editor.submit(input).unwrap();
    }
}

#[test]
fn layer_dialogue_handles_lists_colors_report_and_one_undo_per_mutation() {
    let mut editor = Editor::default();
    let original = editor.drawing().clone();
    submit(&mut editor, &["LAYER 2", "LAYER COLOR 3"]);
    assert_eq!(editor.drawing().header.layers[&2], 3);
    submit(&mut editor, &["LAYER", "OFF", "1, 2,2"]);
    assert_eq!(
        editor
            .drawing()
            .header
            .off_layers
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(editor.drawing().header.current_layer, 2);
    let hidden = editor.drawing().clone();
    let Effect::Report(report) = editor.submit("LAYER ?").unwrap() else {
        panic!("report");
    };
    assert!(report.contains("2  3  OFF  current"));
    assert_eq!(editor.drawing(), &hidden);
    submit(&mut editor, &["LAYER OFF 1,2", "UNDO"]);
    assert!(
        editor.drawing().header.off_layers.is_empty(),
        "report/no-op adds no undo"
    );
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing().header.layers[&2], 15);
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing(), &original);
}

#[test]
fn layer_lists_validate_atomically_and_retry_in_same_prompt() {
    let mut editor = Editor::default();
    let before = editor.drawing().clone();
    assert!(editor.submit("LAYER OFF 1,128").is_err());
    assert_eq!(editor.drawing(), &before);
    assert!(editor.prompt().contains("LAYER OFF"));
    assert!(editor.submit("1,9").unwrap_err().contains("undefined"));
    assert_eq!(editor.drawing(), &before);
    assert!(editor.submit("1,").is_err());
    editor.submit("1").unwrap();
    assert!(!editor.drawing().header.layer_is_visible(1));
    submit(&mut editor, &["LAYER 1", "POINT", "1,2"]);
    assert_eq!(
        editor.drawing().entities().count(),
        1,
        "hidden current layer permits creation by native policy"
    );
    submit(&mut editor, &["LAYER ON 1", "LAYER COLOR", "4"]);
    assert!(editor.drawing().header.layer_is_visible(1));
    assert_eq!(editor.drawing().header.layers[&1], 4);
}

#[test]
fn legacy_color_dialogue_and_numeric_layers_remain_available_and_undoable() {
    let mut editor = Editor::default();
    submit(&mut editor, &["COLOR", "5", "2"]);
    assert_eq!(editor.drawing().header.layers[&1], 5);
    assert_eq!(editor.drawing().header.current_layer, 2);
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing().header.current_layer, 1);
    submit(&mut editor, &["UNDO"]);
    assert_eq!(editor.drawing().header.layers[&1], 15);
}

#[test]
fn historical_dwg_writer_roundtrips_supported_signed_off_state() {
    let mut editor = Editor::default();
    submit(&mut editor, &["LAYER OFF 1"]);
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let drawing =
            acad_dwg::parse(&acad_dwg::write_version(editor.drawing(), version).unwrap()).unwrap();
        assert_eq!(
            drawing.header.off_layers,
            editor.drawing().header.off_layers
        );
        assert_eq!(drawing.header.layers, editor.drawing().header.layers);
        assert!(!drawing.header.layer_is_visible(1));
    }
    submit(&mut editor, &["UNDO"]);
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let drawing =
            acad_dwg::parse(&acad_dwg::write_version(editor.drawing(), version).unwrap()).unwrap();
        assert!(drawing.header.off_layers.is_empty());
        assert_eq!(drawing.header.layers, editor.drawing().header.layers);
    }
}

#[test]
fn bare_record_visibility_matches_historical_dwg_default_layer_on_reopen() {
    use acad_model::{Entity, Item, Point};
    let mut editor = Editor::default();
    editor.drawing_mut().items.push(Item::Entity(Entity::Line {
        start: Point { x: 1.0, y: 1.0 },
        end: Point { x: 2.0, y: 2.0 },
    }));
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let mut reopened =
            acad_dwg::parse(&acad_dwg::write_version(editor.drawing(), version).unwrap()).unwrap();
        assert!(matches!(
            &reopened.items[0],
            Item::Entity(Entity::OnLayer { layer: 1, .. })
        ));
        for layer in [0, 1] {
            let mut native = editor.drawing().clone();
            native.header.off_layers.insert(layer);
            reopened.header.off_layers.clear();
            reopened.header.off_layers.insert(layer);
            assert_eq!(
                native.header.item_is_visible(&native.items[0]),
                reopened.header.item_is_visible(&reopened.items[0])
            );
            assert_eq!(native.header.item_is_visible(&native.items[0]), layer == 0);
        }
    }
}
