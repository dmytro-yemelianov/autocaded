use acad_cmd::{Editor, MenuControl};
use acad_model::{Entity, Point};

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn inputs(editor: &mut Editor, values: &[&str]) {
    for value in values {
        editor.submit(value).unwrap();
    }
}
fn bare(entity: &Entity) -> &Entity {
    match entity {
        Entity::OnLayer { entity, .. } => bare(entity),
        other => other,
    }
}
fn line(editor: &Editor, index: usize) -> (Point, Point) {
    match bare(editor.drawing().entities().nth(index).unwrap()) {
        Entity::Line { start, end } => (*start, *end),
        other => panic!("not a line: {other:?}"),
    }
}

#[test]
fn snap_places_mouse_points_on_the_world_grid_with_signed_half_steps() {
    let mut editor = Editor::default();
    inputs(&mut editor, &["SNAP", "0.5"]);
    for (raw, expected) in [
        (p(0.74, -0.76), p(0.5, -1.0)),
        (p(1.25, -1.25), p(1.5, -1.5)),
    ] {
        editor.submit("POINT").unwrap();
        let before = editor.drawing().clone();
        assert_eq!(editor.constrain_mouse_point(raw).unwrap(), expected);
        assert_eq!(editor.drawing(), &before, "preview must be read-only");
        editor.submit_mouse_point(raw).unwrap();
        assert!(
            matches!(bare(editor.drawing().entities().last().unwrap()),Entity::Point{origin} if *origin==expected)
        );
    }
}

#[test]
fn ortho_tracks_each_line_vertex_and_keeps_first_points_free() {
    let mut editor = Editor::default();
    inputs(&mut editor, &["ORTHO", "ON", "LINE", "1.2,2.3"]);
    editor.submit_mouse_point(p(4.0, 3.0)).unwrap();
    assert_eq!(line(&editor, 0), (p(1.2, 2.3), p(4.0, 2.3)));
    editor.submit_mouse_point(p(4.5, 6.0)).unwrap();
    assert_eq!(line(&editor, 1), (p(4.0, 2.3), p(4.0, 6.0)));
    editor.submit_mouse_point(p(5.0, 7.0)).unwrap();
    assert_eq!(
        line(&editor, 2),
        (p(4.0, 6.0), p(5.0, 6.0)),
        "horizontal wins ties"
    );
    inputs(&mut editor, &["", "LINE"]);
    assert_eq!(
        editor.constrain_mouse_point(p(-2.1, 3.7)).unwrap(),
        p(-2.1, 3.7)
    );
}

#[test]
fn snap_and_ortho_preserve_connections_to_off_grid_typed_anchors() {
    let mut editor = Editor::default();
    inputs(
        &mut editor,
        &["SNAP", "1", "ORTHO", "ON", "LINE", "1.2,2.3"],
    );
    editor.submit_mouse_point(p(4.6, 3.49)).unwrap();
    assert_eq!(line(&editor, 0), (p(1.2, 2.3), p(5.0, 2.3)));
    editor.submit_mouse_point(p(5.49, 6.51)).unwrap();
    assert_eq!(line(&editor, 1), (p(5.0, 2.3), p(5.0, 7.0)));
}

#[test]
fn menu_toggles_apply_to_the_pending_point_and_cancel_drops_its_anchor() {
    let mut editor = Editor::default();
    inputs(&mut editor, &["LINE", "1.2,2.3"]);
    let raw = p(4.6, 3.49);
    assert_eq!(editor.constrain_mouse_point(raw).unwrap(), raw);
    editor.apply_menu_control(MenuControl::Snap).unwrap();
    assert_eq!(editor.constrain_mouse_point(raw).unwrap(), p(5.0, 3.0));
    editor.apply_menu_control(MenuControl::Ortho).unwrap();
    assert_eq!(editor.constrain_mouse_point(raw).unwrap(), p(5.0, 2.3));
    editor.apply_menu_control(MenuControl::Snap).unwrap();
    assert_eq!(editor.constrain_mouse_point(raw).unwrap(), p(4.6, 2.3));
    editor.apply_menu_control(MenuControl::Cancel).unwrap();
    assert!(editor.constrain_mouse_point(raw).is_err());
    editor.submit("LINE").unwrap();
    assert_eq!(editor.constrain_mouse_point(raw).unwrap(), raw);
}

#[test]
fn typed_absolute_relative_and_polar_inputs_bypass_mouse_constraints() {
    let mut editor = Editor::default();
    inputs(
        &mut editor,
        &[
            "SNAP",
            "10",
            "ORTHO",
            "ON",
            "LINE",
            "1.25,2.25",
            "@0.5,0.25",
            "@1<45",
            "",
        ],
    );
    assert_eq!(line(&editor, 0), (p(1.25, 2.25), p(1.75, 2.5)));
    let (start, end) = line(&editor, 1);
    assert_eq!(start, p(1.75, 2.5));
    assert!((end.x - start.x - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-10);
    assert!((end.y - start.y - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-10);
}

#[test]
fn move_destination_and_distance_use_their_current_base_point() {
    let mut editor = Editor::default();
    inputs(
        &mut editor,
        &["POINT", "0.25,0.75", "ORTHO", "ON", "MOVE", "0,0"],
    );
    editor.submit_mouse_point(p(3.0, 1.0)).unwrap();
    editor.submit("ALL").unwrap();
    assert!(
        matches!(bare(editor.drawing().entities().next().unwrap()),Entity::Point{origin} if *origin==p(3.25,0.75))
    );
    inputs(&mut editor, &["DIST", "1,1"]);
    assert_eq!(
        editor.constrain_mouse_point(p(5.0, 2.0)).unwrap(),
        p(5.0, 1.0)
    );
    editor.submit_mouse_point(p(5.0, 2.0)).unwrap();
    assert!(editor.status().contains("4.0000"), "{}", editor.status());
}

#[test]
fn trace_area_and_solid_follow_the_most_recent_vertex() {
    for script in [
        vec!["TRACE", "0.5", "1,1"],
        vec!["AREA", "1,1"],
        vec!["SOLID", "1,1"],
    ] {
        let mut editor = Editor::default();
        inputs(&mut editor, &["ORTHO", "ON"]);
        inputs(&mut editor, &script);
        editor.submit_mouse_point(p(4.0, 2.0)).unwrap();
        assert_eq!(
            editor.constrain_mouse_point(p(4.5, 5.0)).unwrap(),
            p(4.0, 5.0)
        );
    }
    let mut editor = Editor::default();
    inputs(&mut editor, &["ORTHO", "ON", "AREA", "0,0"]);
    for raw in [p(4.0, 0.3), p(4.2, 3.0), p(0.0, 3.5)] {
        editor.submit_mouse_point(raw).unwrap();
    }
    editor.submit("").unwrap();
    assert!(editor.status().contains("12.0000"), "{}", editor.status());
}

#[test]
fn curve_dimension_and_two_axis_points_are_not_ortho_projected() {
    for script in [
        vec!["ARC", "0,0"],
        vec!["DIM", "1,1", "5,1"],
        vec!["LIMITS", "0,0"],
        vec![
            "POINT", "0,0", "BLOCK", "B", "0,0", "ALL", "INSERT", "B", "0,0",
        ],
    ] {
        let mut editor = Editor::default();
        inputs(&mut editor, &["ORTHO", "ON"]);
        inputs(&mut editor, &script);
        let raw = p(3.25, 2.5);
        assert_eq!(
            editor.constrain_mouse_point(raw).unwrap(),
            raw,
            "{script:?}"
        );
    }
    let mut editor = Editor::default();
    inputs(&mut editor, &["ORTHO", "ON", "DIM", "1,1", "5,1"]);
    editor.submit_mouse_point(p(3.0, 2.0)).unwrap();
    editor.submit("").unwrap();
    assert_eq!(editor.drawing().entities().count(), 7);
}

#[test]
fn view_and_hatch_selection_windows_remain_free_with_both_modes_on() {
    let mut editor = Editor::default();
    inputs(&mut editor, &["SNAP", "10", "ORTHO", "ON", "ZOOM", "W"]);
    let a = p(1.13, 1.17);
    let b = p(3.44, 4.66);
    assert_eq!(editor.constrain_mouse_point(a).unwrap(), a);
    editor.submit_mouse_point(a).unwrap();
    assert_eq!(editor.constrain_mouse_point(b).unwrap(), b);
    editor.submit_mouse_point(b).unwrap();
    let center = editor.drawing().header.view.center;
    assert!((center.x - (a.x + b.x) / 2.0).abs() < 1e-10);
    assert!((center.y - (a.y + b.y) / 2.0).abs() < 1e-10);
    inputs(
        &mut editor,
        &[
            "LINE", "0,0", "4,0", "4,4", "0,4", "0,0", "", "HATCH", "NET", "", "", "W",
        ],
    );
    editor.submit_mouse_point(p(-0.1, -0.1)).unwrap();
    editor.submit_mouse_point(p(4.1, 4.1)).unwrap();
    assert_eq!(editor.drawing().blocks().next().unwrap().entities.len(), 64);
}

#[test]
fn invalid_mouse_coordinates_and_snap_intervals_do_not_change_the_command() {
    for spacing in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e-320] {
        let mut editor = Editor::default();
        editor.drawing_mut().header.snap.on = true;
        editor.drawing_mut().header.snap.spacing = spacing;
        editor.submit("POINT").unwrap();
        assert!(editor.submit_mouse_point(p(1e300, 2.0)).is_err());
        assert_eq!(editor.prompt(), "POINT: point");
        assert!(editor.drawing().items.is_empty());
    }
    let mut editor = Editor::default();
    editor.submit("POINT").unwrap();
    for raw in [p(f64::NAN, 0.0), p(0.0, f64::INFINITY)] {
        assert!(editor.submit_mouse_point(raw).is_err());
        assert_eq!(editor.prompt(), "POINT: point");
        assert!(editor.drawing().items.is_empty());
    }
    editor.drawing_mut().header.snap.spacing = f64::NAN;
    editor.submit_mouse_point(p(1.25, 2.25)).unwrap();
}
