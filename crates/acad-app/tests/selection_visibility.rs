use acad_app::{api, Session};
use acad_model::Point;
use serde_json::json;
fn call(session: &mut Session, method: &str, params: serde_json::Value) -> serde_json::Value {
    api::dispatch(
        session,
        serde_json::from_value(json!({"method":method,"params":params})).unwrap(),
        (800, 600),
    )
    .unwrap()
}
fn yellow(pixels: &[u8]) -> usize {
    pixels
        .chunks_exact(4)
        // Highlight strokes may blend yellow with their colored underlay.
        .filter(|pixel| pixel[0] > 100 && pixel[1] > 100 && pixel[2] < pixel[0].min(pixel[1]) / 2)
        .count()
}
#[test]
fn api_clicks_skip_hidden_objects_keep_ids_and_share_visible_highlight_without_mutation() {
    let mut session = Session::default();
    for input in [
        "POINT",
        "2,2",
        "LAYER 2",
        "POINT",
        "5,2",
        "LAYER OFF 1",
        "ERASE",
    ] {
        call(&mut session, "command", json!({"input":input}));
    }
    let before = session.drawing().clone();
    let view = before.header.view;
    let vp = acad_render::Viewport::from_view(view.center, view.height, 800, 556);
    for (world, expected) in [
        (Point { x: 2.0, y: 2.0 }, ""),
        (Point { x: 5.0, y: 2.0 }, "2"),
    ] {
        let screen = vp.to_screen(world);
        call(&mut session, "click", json!({"x":screen.x,"y":screen.y}));
        assert_eq!(session.input(), expected);
        assert_eq!(session.drawing(), &before);
    }
    // The full-frame crosshair overlays a POINT marker at its pick location.
    // Move it to empty space while retaining the selected ID before inspecting pixels.
    let empty = vp.to_screen(Point { x: 10.0, y: 8.0 });
    call(&mut session, "click", json!({"x":empty.x,"y":empty.y}));
    assert_eq!(session.input(), "2");
    assert!(yellow(&session.frame(800, 600).unwrap().rgba()) > 0);
    session.set_input("1".into());
    assert_eq!(
        yellow(&session.frame(800, 600).unwrap().rgba()),
        0,
        "hidden explicit ID receives no highlight"
    );
    assert_eq!(session.drawing(), &before);
    call(&mut session, "command", json!({"input":"1"}));
    call(&mut session, "command", json!({"input":"UNDO"}));
    assert_eq!(
        session.drawing(),
        &before,
        "explicit hidden ID remains editable and undoable"
    );
}
