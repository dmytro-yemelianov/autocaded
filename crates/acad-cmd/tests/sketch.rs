//! Native mouse SKETCH contract (docs/native-sketch.md). Observed rows O1–O14
//! are anchored by crates/acad-oracle/tests/sketch_mouse.rs.
use acad_cmd::{Editor, MenuControl, SketchMode, MAX_SKETCH_SEGMENTS};
use acad_model::{Entity, Item, Point};

const PROMPT: &str = "Sketch.  Pen eXit Quit Record Erase Connect .";

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

fn sketch(increment: &str) -> Editor {
    let mut editor = Editor::default();
    editor.submit("SKETCH").unwrap();
    assert_eq!(editor.prompt(), "SKETCH: record increment");
    editor.submit(increment).unwrap();
    assert_eq!(editor.prompt(), PROMPT);
    editor
}

fn moves(editor: &mut Editor, points: &[(f64, f64)]) {
    for &(x, y) in points {
        editor.pointer_moved(p(x, y)).unwrap();
    }
}

/// Click = pen toggle at the given point (O2).
fn click(editor: &mut Editor, x: f64, y: f64) {
    editor.submit_mouse_point(p(x, y)).unwrap();
}

fn lines(editor: &Editor) -> Vec<(Point, Point)> {
    editor
        .drawing()
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Entity(Entity::OnLayer { entity, .. }) => match entity.as_ref() {
                Entity::Line { start, end } => Some((*start, *end)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn temporary(editor: &Editor) -> Vec<[Point; 2]> {
    editor.sketch_preview().unwrap().temporary
}

#[test]
fn increment_validation_and_prompt() {
    let mut editor = Editor::default();
    editor.submit("SKETCH").unwrap();
    for bad in ["0", "-1", "nan", "inf", "x", ""] {
        assert!(editor.submit(bad).is_err(), "{bad}");
        assert_eq!(editor.prompt(), "SKETCH: record increment");
    }
    editor.submit("0.5").unwrap();
    assert_eq!(editor.prompt(), PROMPT);
    assert!(editor.accepts_mouse_point());
    let preview = editor.sketch_preview().unwrap();
    assert!(!preview.pen_down);
    assert_eq!(preview.mode, SketchMode::Draw);
    assert!(preview.temporary.is_empty());
    assert!(Editor::default().sketch_preview().is_none());
}

#[test]
fn chebyshev_increment_sampling_and_pen_up_tail() {
    let mut editor = sketch("1");
    click(&mut editor, 0.0, 0.0);
    assert!(editor.sketch_preview().unwrap().pen_down);
    // Euclidean 1.2 but neither axis reaches the increment: no vertex (O3).
    moves(&mut editor, &[(0.85, 0.85), (0.5, 0.9)]);
    assert!(temporary(&editor).is_empty());
    // One axis reaches it: vertex at the sampled point.
    moves(&mut editor, &[(0.2, 1.0)]);
    assert_eq!(temporary(&editor), vec![[p(0.0, 0.0), p(0.2, 1.0)]]);
    // Sub-increment motion, then pen up records the exact tail (O4).
    moves(&mut editor, &[(0.5, 1.3)]);
    assert_eq!(temporary(&editor).len(), 1);
    click(&mut editor, 0.6, 1.4);
    assert!(!editor.sketch_preview().unwrap().pen_down);
    assert_eq!(
        temporary(&editor),
        vec![[p(0.0, 0.0), p(0.2, 1.0)], [p(0.2, 1.0), p(0.6, 1.4)]]
    );
    // Pen-up motion draws nothing.
    moves(&mut editor, &[(5.0, 5.0), (8.0, 1.0)]);
    assert_eq!(temporary(&editor).len(), 2);
    assert!(
        lines(&editor).is_empty(),
        "temporary lines stay out of the drawing"
    );
}

#[test]
fn p_key_toggles_like_click_and_requires_a_pointer() {
    let mut editor = sketch("1");
    assert!(editor.submit("P").is_err(), "no pointer position yet");
    assert_eq!(editor.prompt(), PROMPT);
    moves(&mut editor, &[(1.0, 1.0)]);
    editor.submit("p").unwrap();
    assert!(editor.sketch_preview().unwrap().pen_down);
    moves(&mut editor, &[(1.0, 3.0)]);
    editor.submit("P").unwrap();
    assert!(!editor.sketch_preview().unwrap().pen_down);
    assert_eq!(temporary(&editor), vec![[p(1.0, 1.0), p(1.0, 3.0)]]);
}

#[test]
fn collinear_same_direction_segments_merge_but_reversals_do_not() {
    let mut editor = sketch("0.5");
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(1.0, 0.0), (2.0, 0.0), (3.0, 0.0)]);
    assert_eq!(temporary(&editor), vec![[p(0.0, 0.0), p(3.0, 0.0)]]);
    moves(&mut editor, &[(1.0, 0.0)]);
    moves(&mut editor, &[(1.0, 2.0)]);
    click(&mut editor, 1.0, 2.2);
    assert_eq!(
        temporary(&editor),
        vec![
            [p(0.0, 0.0), p(3.0, 0.0)],
            [p(3.0, 0.0), p(1.0, 0.0)],
            [p(1.0, 0.0), p(1.0, 2.2)],
        ]
    );
}

#[test]
fn record_keeps_sketching_from_the_tail_and_reports_counts() {
    let mut editor = sketch("0.25");
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(0.1, 0.0)]);
    editor.submit("R").unwrap();
    assert_eq!(editor.status(), "1 lines recorded.");
    assert_eq!(editor.prompt(), PROMPT);
    assert_eq!(lines(&editor), vec![(p(0.0, 0.0), p(0.1, 0.0))]);
    assert!(temporary(&editor).is_empty());
    assert!(editor.sketch_preview().unwrap().pen_down);
    moves(&mut editor, &[(0.2, 0.0)]);
    editor.submit("X").unwrap();
    assert_eq!(editor.status(), "1 lines recorded.");
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(
        lines(&editor),
        vec![(p(0.0, 0.0), p(0.1, 0.0)), (p(0.1, 0.0), p(0.2, 0.0))]
    );
    // Recorded lines use the current layer.
    let mut editor = Editor::default();
    editor.drawing_mut().header.current_layer = 4;
    editor.drawing_mut().header.layers.insert(4, 3);
    editor.submit("SKETCH").unwrap();
    editor.submit("1").unwrap();
    click(&mut editor, 0.0, 0.0);
    click(&mut editor, 2.0, 0.0);
    editor.submit("").unwrap();
    assert_eq!(editor.status(), "1 lines recorded.");
    assert_eq!(editor.prompt(), "Command");
    assert!(matches!(
        &editor.drawing().items[0],
        Item::Entity(Entity::OnLayer { layer: 4, .. })
    ));
}

#[test]
fn quit_and_cancel_discard_only_temporary_lines() {
    for exit in ["Q", "esc", "menu"] {
        let mut editor = sketch("0.5");
        click(&mut editor, 0.0, 0.0);
        moves(&mut editor, &[(1.0, 0.0), (1.0, 1.0)]);
        editor.submit("R").unwrap();
        assert_eq!(editor.status(), "2 lines recorded.");
        moves(&mut editor, &[(3.0, 1.0), (3.0, 4.0)]);
        assert_eq!(temporary(&editor).len(), 2);
        match exit {
            "Q" => {
                editor.submit("q").unwrap();
            }
            "esc" => {
                editor.cancel_command().unwrap();
            }
            _ => {
                editor.apply_menu_control(MenuControl::Cancel).unwrap();
            }
        }
        assert_eq!(editor.prompt(), "Command", "{exit}");
        assert!(editor.sketch_preview().is_none());
        assert_eq!(lines(&editor).len(), 2, "{exit}");
        editor.submit("UNDO").unwrap();
        assert!(lines(&editor).is_empty(), "{exit}: one batch, one undo");
    }
}

#[test]
fn dot_draws_from_last_end_with_pen_up_only() {
    let mut editor = sketch("0.25");
    moves(&mut editor, &[(1.0, 1.0)]);
    editor.submit(".").unwrap();
    assert_eq!(editor.status(), "No last point known.");
    assert!(temporary(&editor).is_empty());
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(2.0, 0.0)]);
    // Pen down: '.' does nothing (O9).
    editor.submit(".").unwrap();
    assert_eq!(temporary(&editor), vec![[p(0.0, 0.0), p(2.0, 0.0)]]);
    click(&mut editor, 2.0, 0.0);
    moves(&mut editor, &[(4.0, 3.0)]);
    editor.submit(".").unwrap();
    assert!(!editor.sketch_preview().unwrap().pen_down);
    assert_eq!(
        temporary(&editor),
        vec![[p(0.0, 0.0), p(2.0, 0.0)], [p(2.0, 0.0), p(4.0, 3.0)]]
    );
    // The new end point becomes the last point for connect.
    moves(&mut editor, &[(9.0, 9.0)]);
    editor.submit("C").unwrap();
    assert_eq!(editor.sketch_preview().unwrap().mode, SketchMode::Connect);
    moves(&mut editor, &[(4.1, 3.1)]);
    let preview = editor.sketch_preview().unwrap();
    assert_eq!(preview.mode, SketchMode::Draw);
    assert!(preview.pen_down);
    moves(&mut editor, &[(4.1, 5.0)]);
    assert_eq!(temporary(&editor)[2], [p(4.0, 3.0), p(4.1, 5.0)]);
}

#[test]
fn connect_messages_tolerance_and_abort() {
    let mut editor = sketch("0.25");
    moves(&mut editor, &[(1.0, 1.0)]);
    editor.submit("C").unwrap();
    assert_eq!(editor.status(), "No last point known.  Connect aborted.");
    assert_eq!(editor.sketch_preview().unwrap().mode, SketchMode::Draw);
    click(&mut editor, 0.0, 0.0);
    editor.submit("C").unwrap();
    assert_eq!(
        editor.status(),
        "Connect command meaningless when pen down.  Connect aborted."
    );
    click(&mut editor, 2.0, 0.0);
    moves(&mut editor, &[(5.0, 5.0)]);
    editor.submit("C").unwrap();
    assert_eq!(editor.status(), "Connect:  Move to endpoint of line.");
    moves(&mut editor, &[(2.3, 0.0)]);
    assert_eq!(editor.sketch_preview().unwrap().mode, SketchMode::Connect);
    moves(&mut editor, &[(2.2, 0.1)]);
    let preview = editor.sketch_preview().unwrap();
    assert_eq!(preview.mode, SketchMode::Draw);
    assert!(preview.pen_down);
    // Connect resumes at the exact end point, not the pointer.
    moves(&mut editor, &[(2.0, 1.0)]);
    assert_eq!(
        temporary(&editor).last().unwrap(),
        &[p(2.0, 0.0), p(2.0, 1.0)]
    );
    // Already at the end point: connects at once. C again aborts.
    click(&mut editor, 2.0, 1.0);
    editor.submit("C").unwrap();
    assert!(editor.sketch_preview().unwrap().pen_down);
    click(&mut editor, 2.0, 1.0);
    moves(&mut editor, &[(7.0, 7.0)]);
    editor.submit("C").unwrap();
    editor.submit("C").unwrap();
    assert_eq!(editor.status(), "Connect aborted.");
    assert_eq!(editor.sketch_preview().unwrap().mode, SketchMode::Draw);
}

#[test]
fn erase_cuts_back_through_the_nearest_vertex() {
    let path = [
        [p(0.0, 0.0), p(2.0, 0.0)],
        [p(2.0, 0.0), p(4.0, 1.0)],
        [p(4.0, 1.0), p(6.0, 0.0)],
    ];
    let drawn = || {
        let mut editor = sketch("0.25");
        click(&mut editor, 0.0, 0.0);
        moves(&mut editor, &[(2.0, 0.0), (4.0, 1.0), (6.0, 0.0)]);
        click(&mut editor, 6.0, 0.0);
        assert_eq!(temporary(&editor), path.to_vec());
        editor
    };
    // (4.05, 1.0) is nearer the third line's interior than the shared
    // vertex (4, 1), yet the vertex rule still erases the second line (O12).
    for (pointer, kept) in [
        ((5.5, 0.3), 2),
        ((4.0, 1.0), 1),
        ((4.05, 1.0), 1),
        ((0.5, 0.1), 0),
    ] {
        let mut editor = drawn();
        editor.submit("E").unwrap();
        assert_eq!(editor.status(), "Erase:  Select end of delete.");
        moves(&mut editor, &[pointer]);
        let preview = editor.sketch_preview().unwrap();
        assert_eq!(preview.mode, SketchMode::Erase);
        assert_eq!(preview.erase_from, Some(kept));
        assert_eq!(preview.temporary.len(), 3, "preview erases nothing yet");
        editor.submit("P").unwrap();
        assert_eq!(temporary(&editor), path[..kept].to_vec());
        let preview = editor.sketch_preview().unwrap();
        assert_eq!(preview.mode, SketchMode::Draw);
        assert!(!preview.pen_down);
        // The last end point becomes the start of the first erased line.
        moves(&mut editor, &[(9.0, 9.0)]);
        editor.submit(".").unwrap();
        assert_eq!(temporary(&editor).last().unwrap()[0], path[kept][0]);
    }
    // E again aborts; E with the pen down records the tail first.
    let mut editor = drawn();
    editor.submit("E").unwrap();
    moves(&mut editor, &[(0.5, 0.1)]);
    editor.submit("e").unwrap();
    assert_eq!(editor.status(), "Erase aborted.");
    assert_eq!(temporary(&editor), path.to_vec());
    click(&mut editor, 6.0, 0.0);
    moves(&mut editor, &[(6.0, 0.1)]);
    editor.submit("E").unwrap();
    assert!(!editor.sketch_preview().unwrap().pen_down);
    assert_eq!(temporary(&editor).len(), 4);
    // Erase with nothing temporary.
    let mut editor = sketch("1");
    editor.submit("E").unwrap();
    assert_eq!(editor.status(), "No temporary lines to erase.");
    assert_eq!(editor.sketch_preview().unwrap().mode, SketchMode::Draw);
}

#[test]
fn other_controls_abort_sub_modes_then_apply() {
    let mut editor = sketch("0.25");
    click(&mut editor, 0.0, 0.0);
    click(&mut editor, 2.0, 0.0);
    editor.submit("E").unwrap();
    editor.submit("X").unwrap();
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.status(), "Erase aborted.  1 lines recorded.");
    assert_eq!(lines(&editor).len(), 1);
    let mut editor = sketch("0.25");
    click(&mut editor, 0.0, 0.0);
    click(&mut editor, 2.0, 0.0);
    moves(&mut editor, &[(5.0, 5.0)]);
    editor.submit("C").unwrap();
    editor.submit("P").unwrap();
    assert_eq!(editor.status(), "Connect aborted.");
    assert!(!editor.sketch_preview().unwrap().pen_down);
}

#[test]
fn unknown_input_is_rejected_without_state_change() {
    let mut editor = sketch("0.25");
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(1.0, 0.0)]);
    let before = editor.sketch_preview().unwrap();
    for input in ["Z", "1,1", "LINE", "PP"] {
        assert!(editor.submit(input).is_err(), "{input}");
        assert_eq!(editor.prompt(), PROMPT);
        assert_eq!(editor.sketch_preview().unwrap(), before);
    }
    assert!(editor.pointer_moved(p(f64::NAN, 0.0)).is_err());
    assert_eq!(editor.sketch_preview().unwrap(), before);
}

#[test]
fn snap_and_ortho_constrain_samples() {
    let mut editor = sketch("0.5");
    editor.drawing_mut().header.snap.on = true;
    editor.drawing_mut().header.snap.spacing = 0.5;
    click(&mut editor, 0.1, 0.2);
    moves(&mut editor, &[(1.4, 0.3)]);
    assert_eq!(temporary(&editor), vec![[p(0.0, 0.0), p(1.5, 0.5)]]);
    assert_eq!(
        editor.constrain_mouse_point(p(2.2, 2.2)).unwrap(),
        p(2.0, 2.0)
    );
    let mut editor = sketch("0.5");
    editor.drawing_mut().header.ortho = true;
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(2.0, 0.3)]);
    moves(&mut editor, &[(2.1, 2.0)]);
    moves(&mut editor, &[(2.1, -1.0)]);
    assert_eq!(
        temporary(&editor),
        vec![
            [p(0.0, 0.0), p(2.0, 0.0)],
            [p(2.0, 0.0), p(2.0, 2.0)],
            [p(2.0, 2.0), p(2.0, -1.0)],
        ]
    );
    // The crosshair previews the ORTHO-constrained pen point.
    assert_eq!(
        editor.constrain_mouse_point(p(3.0, -0.8)).unwrap(),
        p(3.0, -1.0)
    );
    // Pen up: SNAP only.
    click(&mut editor, 3.0, -0.8);
    assert_eq!(temporary(&editor)[3], [p(2.0, -1.0), p(3.0, -1.0)]);
    assert_eq!(
        editor.constrain_mouse_point(p(5.0, 4.0)).unwrap(),
        p(5.0, 4.0)
    );
}

#[test]
fn temporary_segments_are_bounded() {
    let mut editor = sketch("1");
    click(&mut editor, 0.0, 0.0);
    // Alternate directions so nothing merges.
    for i in 1..=MAX_SKETCH_SEGMENTS + 5 {
        let x = i as f64;
        let y = if i % 2 == 0 { 0.0 } else { 2.0 };
        editor.pointer_moved(p(x, y)).unwrap();
    }
    assert_eq!(temporary(&editor).len(), MAX_SKETCH_SEGMENTS);
    assert!(
        editor.status().contains("buffer full"),
        "{}",
        editor.status()
    );
    // R/X with the pen down never drop the tail: it joins the batch.
    let mut exiting = editor.clone();
    let pointer = p((MAX_SKETCH_SEGMENTS + 5) as f64, 2.0);
    exiting.submit("X").unwrap();
    assert_eq!(
        exiting.status(),
        format!("{} lines recorded.", MAX_SKETCH_SEGMENTS + 1)
    );
    assert_eq!(lines(&exiting).last().unwrap().1, pointer);
    // A pen-up with a full buffer cannot keep its tail; the status says so.
    click(&mut editor, 0.0, 50.0);
    assert!(!editor.sketch_preview().unwrap().pen_down);
    assert!(editor.status().contains("buffer full"));
    assert_eq!(temporary(&editor).len(), MAX_SKETCH_SEGMENTS);
    editor.submit("R").unwrap();
    assert_eq!(
        editor.status(),
        format!("{MAX_SKETCH_SEGMENTS} lines recorded.")
    );
    assert!(temporary(&editor).is_empty());
    assert_eq!(lines(&editor).len(), MAX_SKETCH_SEGMENTS);
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(0.0, 3.0)]);
    assert_eq!(temporary(&editor).len(), 1);
}

#[test]
fn one_undo_per_record_batch() {
    let mut editor = sketch("0.5");
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(1.0, 0.0), (1.0, 1.0)]);
    editor.submit("R").unwrap();
    moves(&mut editor, &[(2.0, 1.0), (2.0, 3.0)]);
    editor.submit("R").unwrap();
    // A record of zero lines takes no snapshot.
    editor.submit("R").unwrap();
    assert_eq!(editor.status(), "0 lines recorded.");
    moves(&mut editor, &[(4.0, 3.0)]);
    editor.submit("X").unwrap();
    assert_eq!(lines(&editor).len(), 5);
    editor.submit("UNDO").unwrap();
    assert_eq!(lines(&editor).len(), 4);
    editor.submit("UNDO").unwrap();
    assert_eq!(lines(&editor).len(), 2);
    editor.submit("UNDO").unwrap();
    assert!(lines(&editor).is_empty());
}

#[test]
fn pointer_motion_outside_sketch_is_ignored() {
    let mut editor = Editor::default();
    editor.submit("LINE").unwrap();
    let before = editor.drawing().clone();
    editor.pointer_moved(p(1.0, 1.0)).unwrap();
    assert_eq!(editor.prompt(), "LINE: first point");
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn click_in_connect_mode_connects_within_tolerance_else_aborts() {
    // Within the tolerance the click's motion connects, then the click lifts
    // the pen, recording the line from the end point to the click.
    let mut editor = sketch("1");
    click(&mut editor, 0.0, 0.0);
    click(&mut editor, 3.0, 0.0);
    moves(&mut editor, &[(9.0, 9.0)]);
    editor.submit("C").unwrap();
    click(&mut editor, 3.5, 0.0);
    let preview = editor.sketch_preview().unwrap();
    assert_eq!(preview.mode, SketchMode::Draw);
    assert!(!preview.pen_down);
    assert_eq!(preview.temporary, vec![[p(0.0, 0.0), p(3.5, 0.0)]]);
    assert_eq!(editor.status(), "");
    // Outside the tolerance the click aborts connect and changes nothing else.
    moves(&mut editor, &[(9.0, 9.0)]);
    editor.submit("C").unwrap();
    click(&mut editor, 8.0, 8.0);
    let preview = editor.sketch_preview().unwrap();
    assert_eq!(editor.status(), "Connect aborted.");
    assert!(!preview.pen_down);
    assert_eq!(preview.temporary.len(), 1);
}

#[test]
fn declined_quit_resumes_sketch_with_temporary_lines() {
    let mut editor = sketch("1");
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(2.0, 0.0)]);
    let before = editor.sketch_preview().unwrap();
    editor.request_quit().unwrap();
    assert!(editor.sketch_preview().is_none());
    assert_eq!(editor.submit("N").unwrap(), acad_cmd::Effect::Continue);
    assert_eq!(editor.prompt(), PROMPT);
    assert_eq!(editor.sketch_preview().unwrap(), before);
    editor.request_quit().unwrap();
    assert_eq!(editor.submit("Y").unwrap(), acad_cmd::Effect::Quit);
    // Cancelling the confirmation discards the suspended strokes.
    let mut editor = sketch("1");
    click(&mut editor, 0.0, 0.0);
    editor.request_quit().unwrap();
    editor.cancel_command().unwrap();
    editor.submit("N").unwrap_err();
    assert_eq!(editor.prompt(), "Command");
    assert!(editor.sketch_preview().is_none());
}

#[test]
fn ortho_pen_up_tail_is_an_axis_aligned_l_to_the_pointer() {
    let mut editor = sketch("1");
    editor.drawing_mut().header.ortho = true;
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(2.0, 0.0), (2.2, 0.6)]);
    click(&mut editor, 2.2, 0.6);
    assert_eq!(
        temporary(&editor),
        vec![
            [p(0.0, 0.0), p(2.0, 0.0)],
            [p(2.0, 0.0), p(2.0, 0.6)],
            [p(2.0, 0.6), p(2.2, 0.6)],
        ]
    );
    // The next '.' starts from the pointer the pen was lifted at.
    moves(&mut editor, &[(5.0, 5.0)]);
    editor.submit(".").unwrap();
    assert_eq!(temporary(&editor)[3][0], p(2.2, 0.6));
}

#[test]
fn ortho_record_and_exit_tails_are_the_same_l_and_r_continues_from_the_pointer() {
    // R: vertical-dominant tail, then continue from the pointer.
    let mut editor = sketch("1");
    editor.drawing_mut().header.ortho = true;
    click(&mut editor, 0.0, 0.0);
    moves(&mut editor, &[(2.0, 0.0), (2.2, 0.6)]);
    let preview = editor.sketch_preview().unwrap();
    assert_eq!(
        preview.rubber,
        vec![[p(2.0, 0.0), p(2.0, 0.6)], [p(2.0, 0.6), p(2.2, 0.6)]],
        "the rubber band previews both legs"
    );
    editor.submit("R").unwrap();
    assert_eq!(editor.status(), "3 lines recorded.");
    assert_eq!(
        lines(&editor),
        vec![
            (p(0.0, 0.0), p(2.0, 0.0)),
            (p(2.0, 0.0), p(2.0, 0.6)),
            (p(2.0, 0.6), p(2.2, 0.6)),
        ]
    );
    moves(&mut editor, &[(2.2, 3.0)]);
    assert_eq!(temporary(&editor), vec![[p(2.2, 0.6), p(2.2, 3.0)]]);
    // X: horizontal-dominant tail.
    moves(&mut editor, &[(2.6, 3.3)]);
    editor.submit("X").unwrap();
    assert_eq!(
        &lines(&editor)[3..],
        &[
            (p(2.2, 0.6), p(2.2, 3.0)),
            (p(2.2, 3.0), p(2.6, 3.0)),
            (p(2.6, 3.0), p(2.6, 3.3)),
        ]
    );
}
