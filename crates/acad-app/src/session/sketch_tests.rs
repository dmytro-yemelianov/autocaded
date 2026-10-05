//! Session-level SKETCH routing (docs/native-sketch.md): GUI pointer motion,
//! immediate control keys, frame-only temporary strokes and crosshair policy.
use super::*;

const W: u32 = 800;
const H: u32 = 600;

fn world(app: &Session, x: f64, y: f64) -> acad_model::Point {
    viewport_for(app.drawing(), W, H).to_world(acad_model::Point { x, y })
}

fn sketching(increment: &str) -> Session {
    let mut app = Session::default();
    app.command("SKETCH").unwrap();
    app.command(increment).unwrap();
    assert!(app.sketch_active());
    app
}

fn hover(app: &mut Session, x: f64, y: f64) {
    app.cursor(Some((x, y)));
    app.pointer_motion(W, H).unwrap();
}

/// Cyan (equal green/blue, no red) pixels, including antialiased edges;
/// the green crosshair and white geometry never match.
fn cyan_pixels(app: &Session) -> usize {
    let frame = app.frame(W, H).unwrap();
    frame
        .pixels
        .iter()
        .enumerate()
        .filter(|(index, pixel)| {
            let (x, y) = (index % W as usize, index / W as usize);
            let (r, g, b) = ((*pixel >> 16) & 0xff, (*pixel >> 8) & 0xff, *pixel & 0xff);
            (50..450).contains(&x) && (50..400).contains(&y) && r < 40 && g == b && g >= 0x60
        })
        .count()
}

#[test]
fn gui_motion_samples_world_pixels_and_keys_act_immediately() {
    let mut app = sketching("0.25");
    hover(&mut app, 100.0, 200.0);
    // A typed control acts without Return, as the original keyboard does.
    assert!(!app.type_characters("p").unwrap());
    assert!(app.input().is_empty());
    assert!(app.editor.sketch_preview().unwrap().pen_down);
    hover(&mut app, 200.0, 200.0);
    hover(&mut app, 200.0, 300.0);
    let start = world(&app, 100.0, 200.0);
    let corner = world(&app, 200.0, 200.0);
    let end = world(&app, 200.0, 300.0);
    assert_eq!(
        app.editor.sketch_preview().unwrap().temporary,
        vec![[start, corner], [corner, end]]
    );
    // Temporary strokes are visible in the frame but not in the drawing.
    assert_eq!(app.drawing().entities().count(), 0);
    assert!(!app.is_dirty());
    assert!(cyan_pixels(&app) > 100);
    // Whitespace is ignored; X records and leaves SKETCH.
    app.type_characters(" ").unwrap();
    assert!(app.sketch_active());
    app.type_characters("x").unwrap();
    assert!(!app.sketch_active());
    assert_eq!(app.status(), "2 lines recorded.");
    assert_eq!(app.drawing().entities().count(), 2);
    assert!(app.is_dirty());
    assert_eq!(cyan_pixels(&app), 0);
    // After SKETCH, typing is ordinary command-line input again.
    app.type_characters("p").unwrap();
    assert_eq!(app.input(), "p");
}

#[test]
fn clicks_toggle_the_pen_and_menu_or_command_area_motion_is_ignored() {
    let mut app = sketching("0.25");
    app.click(100.0, 100.0, W, H).unwrap();
    assert!(app.editor.sketch_preview().unwrap().pen_down);
    let before = app.editor.sketch_preview().unwrap();
    // Command area and screen-menu panel motion do not sample.
    hover(&mut app, 300.0, f64::from(H) - 5.0);
    if app.menu.is_some() {
        hover(&mut app, f64::from(W) - 20.0, 100.0);
    }
    assert_eq!(app.editor.sketch_preview().unwrap(), before);
    app.click(300.0, 100.0, W, H).unwrap();
    let preview = app.editor.sketch_preview().unwrap();
    assert!(!preview.pen_down);
    assert_eq!(
        preview.temporary,
        vec![[world(&app, 100.0, 100.0), world(&app, 300.0, 100.0)]]
    );
    // Esc discards temporary strokes.
    app.cancel().unwrap();
    assert!(!app.sketch_active());
    assert_eq!(app.drawing().entities().count(), 0);
    assert!(!app.is_dirty());
}

#[test]
fn crosshair_previews_ortho_constrained_pen_point() {
    let mut app = sketching("0.25");
    app.editor.drawing_mut().header.ortho = true;
    app.click(100.0, 200.0, W, H).unwrap();
    app.cursor(Some((300.0, 210.0)));
    let (x, y) = app.crosshair_position(W, H).unwrap();
    assert!((x - 300.0).abs() < 1e-6);
    assert!((y - 200.0).abs() < 1e-6, "{y}");
    let preview = app.editor.sketch_preview().unwrap();
    assert!(preview.temporary.is_empty());
    app.pointer_motion(W, H).unwrap();
    let preview = app.editor.sketch_preview().unwrap();
    assert_eq!(preview.temporary[0][1].y, world(&app, 100.0, 200.0).y);
}

#[test]
fn motion_outside_sketch_changes_nothing() {
    let mut app = Session::default();
    app.command("LINE").unwrap();
    hover(&mut app, 100.0, 100.0);
    assert_eq!(app.prompt(), "LINE: first point");
    assert_eq!(app.drawing().entities().count(), 0);
}

#[test]
fn modifier_chords_never_act_as_sketch_controls_and_ctrl_c_cancels() {
    let ctrl = KeyModifiers {
        control: true,
        ..KeyModifiers::default()
    };
    let alt = KeyModifiers {
        alt: true,
        ..KeyModifiers::default()
    };
    let logo = KeyModifiers {
        logo: true,
        ..KeyModifiers::default()
    };
    let mut app = sketching("0.25");
    app.click(100.0, 200.0, W, H).unwrap();
    hover(&mut app, 200.0, 200.0);
    let before = app.editor.sketch_preview().unwrap();
    // Ctrl+Q, Alt+X, Cmd+R, Ctrl+P: no Quit, eXit, Record or Pen.
    for (text, modifiers) in [("q", ctrl), ("x", alt), ("r", logo), ("p", ctrl)] {
        assert!(!app.key_character(text, modifiers).unwrap());
        assert!(app.sketch_active(), "{text}");
        assert_eq!(app.editor.sketch_preview().unwrap(), before);
        assert!(app.input().is_empty());
    }
    // Ctrl+C cancels, discarding temporary strokes (O8).
    app.key_character("c", ctrl).unwrap();
    assert!(!app.sketch_active());
    assert_eq!(app.prompt(), "Command");
    assert_eq!(app.drawing().entities().count(), 0);
    // Some platforms deliver Ctrl+C as ETX.
    let mut app = sketching("0.25");
    app.key_character("\u{3}", ctrl).unwrap();
    assert!(!app.sketch_active());
    // Unmodified letters remain immediate controls.
    let mut app = sketching("0.25");
    app.key_character("q", KeyModifiers::default()).unwrap();
    assert!(!app.sketch_active());
    // Outside SKETCH the old text route is unchanged: control characters are
    // dropped and letters are typed.
    app.key_character("\u{3}", ctrl).unwrap();
    assert!(app.input().is_empty());
    app.key_character("l", KeyModifiers::default()).unwrap();
    assert_eq!(app.input(), "l");
}

#[test]
fn window_close_declined_resumes_sketch() {
    let mut app = sketching("0.25");
    app.click(100.0, 200.0, W, H).unwrap();
    hover(&mut app, 200.0, 200.0);
    let before = app.editor.sketch_preview().unwrap();
    assert!(!app.request_quit().unwrap());
    assert!(!app.command("N").unwrap());
    assert!(app.sketch_active());
    assert_eq!(app.editor.sketch_preview().unwrap(), before);
}

fn drag_motion(app: &mut Session, x: f64, y: f64) {
    app.motion(x, y, W, H).unwrap();
}

#[test]
fn gui_press_drag_release_draws_one_stroke() {
    let mut app = sketching("0.25");
    app.press(100.0, 200.0, W, H).unwrap();
    assert!(app.editor.sketch_preview().unwrap().pen_down);
    drag_motion(&mut app, 200.0, 200.0);
    drag_motion(&mut app, 200.0, 300.0);
    app.release(220.0, 300.0, W, H).unwrap();
    let sketch = app.editor.sketch_preview().unwrap();
    assert!(!sketch.pen_down);
    let [start, corner, end, tail] = [
        (100.0, 200.0),
        (200.0, 200.0),
        (200.0, 300.0),
        (220.0, 300.0),
    ]
    .map(|(x, y)| world(&app, x, y));
    // The release records the short tail to the release point (O4).
    assert_eq!(
        sketch.temporary,
        vec![[start, corner], [corner, end], [end, tail]]
    );
    // Moving with the button up draws nothing.
    drag_motion(&mut app, 400.0, 100.0);
    assert_eq!(app.editor.sketch_preview().unwrap().temporary.len(), 3);
    app.type_characters("x").unwrap();
    assert_eq!(app.drawing().entities().count(), 3);
}

#[test]
fn a_second_drag_starts_a_new_stroke_and_release_without_drag_is_ignored() {
    let mut app = sketching("0.25");
    // A stray release (no press) changes nothing.
    app.release(100.0, 100.0, W, H).unwrap();
    assert!(!app.editor.sketch_preview().unwrap().pen_down);
    for (from, to) in [
        ((100.0, 100.0), (300.0, 100.0)),
        ((100.0, 300.0), (300.0, 300.0)),
    ] {
        app.press(from.0, from.1, W, H).unwrap();
        drag_motion(&mut app, to.0, to.1);
        app.release(to.0, to.1, W, H).unwrap();
    }
    assert_eq!(app.editor.sketch_preview().unwrap().temporary.len(), 2);
}

#[test]
fn release_outside_the_drawing_lifts_at_the_last_pointer() {
    let mut app = sketching("0.25");
    app.press(100.0, 100.0, W, H).unwrap();
    drag_motion(&mut app, 300.0, 100.0);
    app.release(f64::NAN, f64::NAN, W, H).unwrap();
    let sketch = app.editor.sketch_preview().unwrap();
    assert!(!sketch.pen_down);
    assert_eq!(
        sketch.temporary,
        vec![[world(&app, 100.0, 100.0), world(&app, 300.0, 100.0)]]
    );
}

#[test]
fn a_press_near_the_last_end_point_connects_and_drags_on() {
    let mut app = sketching("0.25");
    app.press(100.0, 100.0, W, H).unwrap();
    drag_motion(&mut app, 300.0, 100.0);
    app.release(300.0, 100.0, W, H).unwrap();
    // Away from the end point, so C waits instead of connecting at once.
    drag_motion(&mut app, 300.0, 400.0);
    app.type_characters("c").unwrap();
    assert_eq!(
        app.editor.sketch_preview().unwrap().mode,
        acad_cmd::SketchMode::Connect
    );
    // A press away from the end point aborts connect, like P (O10 policy).
    app.press(300.0, 400.0, W, H).unwrap();
    assert_eq!(
        app.editor.sketch_preview().unwrap().mode,
        acad_cmd::SketchMode::Draw
    );
    assert!(!app.editor.sketch_preview().unwrap().pen_down);
    app.release(300.0, 400.0, W, H).unwrap();
    app.type_characters("c").unwrap();
    // A press on the end point connects there and keeps drawing.
    app.press(300.0, 100.0, W, H).unwrap();
    assert!(app.editor.sketch_preview().unwrap().pen_down);
    drag_motion(&mut app, 300.0, 300.0);
    app.release(300.0, 300.0, W, H).unwrap();
    let temporary = app.editor.sketch_preview().unwrap().temporary;
    assert_eq!(temporary.len(), 2);
    assert_eq!(temporary[1][0], world(&app, 300.0, 100.0));
}

#[test]
fn erase_mode_press_confirms_and_menu_presses_stay_clicks() {
    let mut app = sketching("0.25");
    app.press(100.0, 100.0, W, H).unwrap();
    drag_motion(&mut app, 300.0, 100.0);
    app.release(300.0, 100.0, W, H).unwrap();
    app.type_characters("e").unwrap();
    app.press(100.0, 100.0, W, H).unwrap();
    assert!(app.editor.sketch_preview().unwrap().temporary.is_empty());
    app.release(100.0, 100.0, W, H).unwrap();
    assert!(!app.editor.sketch_preview().unwrap().pen_down);
}
