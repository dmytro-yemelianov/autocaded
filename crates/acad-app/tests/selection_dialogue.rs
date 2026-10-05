use acad_app::{api, Session};
use acad_model::Point;
use acad_render::Viewport;
use serde_json::{json, Value};
fn call(s: &mut Session, method: &str, params: Value) -> Result<Value, String> {
    api::dispatch(
        s,
        serde_json::from_value(json!({"method":method,"params":params})).unwrap(),
        (640, 440),
    )
}
fn commands(s: &mut Session, inputs: &[&str]) {
    for input in inputs {
        call(s, "command", json!({"input":input})).unwrap();
    }
}
fn world_click(s: &mut Session, point: Point) {
    let view = s.drawing().header.view;
    let screen = Viewport::from_view(view.center, view.height, 640, 396).to_screen(point);
    call(
        s,
        "click",
        json!({"x":screen.x,"y":screen.y,"width":640,"height":440}),
    )
    .unwrap();
}

#[test]
fn pixel_picks_world_points_and_windows_share_dedup_readonly_selected_list() {
    let mut s = Session::default();
    commands(
        &mut s,
        &[
            "MENU",
            "",
            "LAYER",
            "2",
            "POINT",
            "-3,-1",
            "LAYER",
            "1",
            "LINE",
            "-4,-2",
            "-2,-2",
            "",
            "REPEAT",
            "POINT",
            "-1,-1",
            "ENDREP",
            "2",
            "1",
            "2",
            "0",
            "LINE",
            "4,0",
            "4,4",
            "",
            "BLOCK",
            "B",
            "0,0",
            "LAST",
            "INSERT",
            "B",
            "4,0",
            "1",
            "1",
            "0",
            "POINT",
            "10,10",
            "LAYER OFF 2",
            "ZOOM",
            "C",
            "0,0",
            "20",
            "SNAP",
            "100",
            "ORTHO",
            "ON",
        ],
    );
    let before = s.drawing().clone();
    let dirty = s.is_dirty();
    commands(&mut s, &["LIST"]);
    assert!(s.prompt().starts_with("LIST:") && s.report_text().is_none());
    world_click(&mut s, Point { x: -4.0, y: -2.0 });
    world_click(&mut s, Point { x: -2.0, y: -2.0 });
    call(&mut s, "point", json!({"x":1,"y":-1})).unwrap();
    call(&mut s, "point", json!({"x":4,"y":0})).unwrap();
    assert_eq!(s.input(), "2,3,4");
    // The hidden point has no pick, and a miss must keep the collected set.
    call(&mut s, "point", json!({"x":-3,"y":-1})).unwrap();
    assert_eq!(s.input(), "2,3,4");
    let frame = s.frame(640, 440).unwrap();
    assert!(
        frame.rgba()[..640 * 396 * 4]
            .chunks_exact(4)
            .filter(|pixel| pixel[0] > 100
                && pixel[1] > 100
                && pixel[2] < pixel[0].min(pixel[1]) / 2)
            .count()
            > 20
    );
    commands(&mut s, &["W"]);
    call(&mut s, "point", json!({"x":-5,"y":-3})).unwrap();
    call(&mut s, "point", json!({"x":9,"y":5})).unwrap();
    assert_eq!(s.input(), "2,3,4");
    assert_eq!(s.drawing(), &before);
    commands(&mut s, &[""]);
    assert_eq!(s.status(), "2 LINE, 3 REPEAT, 4 INSERT");
    assert!(s
        .report_text()
        .unwrap()
        .starts_with("                  LINE      LAYER:"));
    assert_eq!(s.prompt(), "Command");
    assert_eq!(s.drawing(), &before);
    assert_eq!(s.is_dirty(), dirty);
    call(&mut s, "report", json!({"action":"close"})).unwrap();
    commands(&mut s, &["LIST", "1"]);
    assert_eq!(s.status(), "1 POINT");
    assert!(s
        .report_text()
        .unwrap()
        .starts_with("                  POINT     LAYER: 2\n"));
    assert_eq!(s.drawing(), &before);
    assert_eq!(s.is_dirty(), dirty);
}

#[test]
fn multiple_world_picks_erase_only_on_return_and_cancel_keeps_undo_unchanged() {
    let mut s = Session::default();
    commands(&mut s, &["POINT", "1,1", "POINT", "3,3", "POINT", "5,5"]);
    let before = s.drawing().clone();
    commands(&mut s, &["ERASE"]);
    call(&mut s, "point", json!({"x":1,"y":1})).unwrap();
    call(&mut s, "point", json!({"x":3,"y":3})).unwrap();
    call(&mut s, "point", json!({"x":1,"y":1})).unwrap();
    assert_eq!(s.input(), "1,2");
    assert_eq!(s.drawing(), &before);
    call(&mut s, "cancel", json!({})).unwrap();
    commands(&mut s, &["UNDO"]);
    assert_eq!(
        s.drawing().entities().count(),
        2,
        "cancel adds no undo entry"
    );
    commands(&mut s, &["ERASE"]);
    call(&mut s, "point", json!({"x":1,"y":1})).unwrap();
    call(&mut s, "point", json!({"x":3,"y":3})).unwrap();
    commands(&mut s, &[""]);
    assert_eq!(s.drawing().entities().count(), 0);
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing().entities().count(), 2);
}

#[test]
fn selected_list_paging_preserves_clean_document_and_geometry_undo_history() {
    let scratch =
        std::env::temp_dir().join(format!("acad-selected-list-{}.dwg", std::process::id()));
    let mut s = Session::default();
    for n in 0..140 {
        commands(&mut s, &["POINT", &format!("{n},1")]);
    }
    s.save(&scratch).unwrap();
    let before = s.drawing().clone();
    commands(&mut s, &["LIST", "ALL"]);
    let text = s.report_text().unwrap().to_owned();
    assert!(text.contains("                at point, X= 139.0000  Y=   1.0000"));
    let first = s.frame(320, 140).unwrap().pixels;
    for action in ["end", "home", "page_down", "close", "open"] {
        call(
            &mut s,
            "report",
            json!({"action":action,"width":320,"height":140}),
        )
        .unwrap();
        assert_eq!(s.report_text(), Some(text.as_str()));
        assert_eq!(s.prompt(), "Command");
        assert_eq!(s.drawing(), &before);
        assert!(!s.is_dirty());
    }
    assert_ne!(s.frame(320, 140).unwrap().pixels, first);
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing().entities().count(), 139);
    assert!(s.is_dirty());
    std::fs::remove_file(&scratch).unwrap();
}

#[test]
fn rejected_hatch_window_keeps_highlighted_collection_for_replacement_and_cancel() {
    let mut s = Session::default();
    commands(&mut s, &["CIRCLE", "0,0", "1", "REPEAT", "POINT", "2,2"]);
    let before_endrep = s.drawing().clone();
    commands(&mut s, &["ENDREP", "2", "1", "1", "0"]);
    let before = s.drawing().clone();
    let dirty = s.is_dirty();
    commands(&mut s, &["HATCH", "LINE", "1", "0", "W", "-2,-2"]);
    assert!(call(&mut s, "command", json!({"input":"4,4"})).is_err());
    assert_eq!(s.input(), "1,2");
    assert!(s.prompt().starts_with("Select objects:"));
    assert_eq!(s.drawing(), &before);
    assert_eq!(s.is_dirty(), dirty);
    commands(&mut s, &["1"]);
    assert_eq!(s.prompt(), "Command");
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &before);
    commands(&mut s, &["HATCH", "LINE", "1", "0", "W", "-2,-2"]);
    assert!(call(&mut s, "command", json!({"input":"4,4"})).is_err());
    call(&mut s, "cancel", json!({})).unwrap();
    commands(&mut s, &["UNDO"]);
    assert_eq!(
        s.drawing(),
        &before_endrep,
        "failed HATCH/cancel added no undo: preceding ENDREP is undone"
    );
}
