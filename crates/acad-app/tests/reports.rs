use acad_app::{api, ReportAction, Session};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};

fn call(s: &mut Session, method: &str, params: Value) -> Result<Value, String> {
    api::dispatch(
        s,
        serde_json::from_value(json!({"method":method,"params":params}))
            .map_err(|e| e.to_string())?,
        (640, 480),
    )
}
fn command(s: &mut Session, input: &str) {
    call(s, "command", json!({"input":input})).unwrap();
}
fn navigate(s: &mut Session, action: &str) -> Value {
    call(
        s,
        "report",
        json!({"action":action,"width":640,"height":240}),
    )
    .unwrap()
}

#[test]
fn complete_reports_wrap_scroll_reopen_and_leave_drawing_and_undo_intact() {
    let mut s = Session::default();
    for input in ["LINE", "1,1", "5,1", "", "HELP", "ZOOM"] {
        command(&mut s, input);
    }
    let before = s.drawing().clone();
    let text = s.report_text().unwrap().to_owned();
    assert!(s.report_visible() && text.ends_with("Reference:  Section 4.3 of User Guide.\n"));
    let first = s.frame(640, 240).unwrap().pixels;
    navigate(&mut s, "page_down");
    let state = api::state(&s);
    assert!(state["report_view"]["anchor"].as_u64().unwrap() > 0);
    assert!(s.frame(640, 240).unwrap().pixels != first);
    navigate(&mut s, "end");
    let last = s.frame(640, 240).unwrap().pixels;
    navigate(&mut s, "page_down");
    assert_eq!(s.frame(640, 240).unwrap().pixels, last);
    // A smaller frame rewraps by source position; every size must remain safe.
    for (w, h) in [(1, 1), (80, 44), (160, 100), (320, 240), (1024, 768)] {
        assert_eq!(s.frame(w, h).unwrap().pixels.len(), (w * h) as usize);
    }
    navigate(&mut s, "home");
    assert_eq!(s.frame(640, 240).unwrap().pixels, first);
    navigate(&mut s, "up");
    assert_eq!(api::state(&s)["report_view"]["anchor"], 0);
    navigate(&mut s, "close");
    assert!(!s.report_visible());
    assert_eq!(s.report_text(), Some(text.as_str()));
    navigate(&mut s, "open");
    assert_eq!(s.frame(640, 240).unwrap().pixels, first);
    assert_eq!(s.drawing(), &before);
    assert!(s.is_dirty());
    command(&mut s, "UNDO");
    assert_eq!(s.drawing().entities().count(), 0);
    assert!(!s.is_dirty() && !s.report_visible() && s.report_text().is_none());
}

#[test]
fn report_pixels_are_shared_by_png_rgba_and_canvas_clicks_only_navigate() {
    let mut s = Session::default();
    for input in ["POINT", "1,1", "POINT", "2,2", "DBLIST"] {
        command(&mut s, input);
    }
    let before = s.drawing().clone();
    let state = api::state(&s);
    call(
        &mut s,
        "click",
        json!({"x":10,"y":40,"width":640,"height":240}),
    )
    .unwrap();
    assert_eq!(s.drawing(), &before);
    assert_eq!(api::state(&s)["report_view"], state["report_view"]);
    let first = s.frame(640, 240).unwrap().rgba();
    let png = call(&mut s, "frame", json!({"width":640,"height":240})).unwrap();
    let data = STANDARD.decode(png["data"].as_str().unwrap()).unwrap();
    let mut reader = png::Decoder::new(data.as_slice()).read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut decoded).unwrap();
    assert_eq!(&decoded[..info.buffer_size()], first);
    let raw = call(
        &mut s,
        "frame",
        json!({"width":640,"height":240,"format":"rgba"}),
    )
    .unwrap();
    assert_eq!(
        STANDARD.decode(raw["data"].as_str().unwrap()).unwrap(),
        first
    );
    // Footer is above the 44-pixel command strip: NEXT, then PREV, then CLOSE.
    call(
        &mut s,
        "click",
        json!({"x":300,"y":180,"width":640,"height":240}),
    )
    .unwrap();
    assert!(s.frame(640, 240).unwrap().rgba() != first);
    call(
        &mut s,
        "click",
        json!({"x":10,"y":180,"width":640,"height":240}),
    )
    .unwrap();
    assert_eq!(s.frame(640, 240).unwrap().rgba(), first);
    call(
        &mut s,
        "click",
        json!({"x":620,"y":180,"width":640,"height":240}),
    )
    .unwrap();
    assert!(!s.report_visible());
    assert_eq!(s.drawing(), &before);
}

#[test]
fn files_menu_keeps_its_pending_prompt_and_typing_hides_the_viewer() {
    let mut s = Session::default();
    command(&mut s, "FILES");
    let prompt = s.prompt().to_owned();
    assert!(s.report_visible());
    navigate(&mut s, "close");
    assert_eq!(s.prompt(), prompt);
    navigate(&mut s, "open");
    s.set_input("L".into());
    assert!(!s.report_visible());
    assert_eq!(s.report_text(), api::state(&s)["report"].as_str());
    s.cancel().unwrap();
    assert_eq!(s.prompt(), "Command");
    assert!(s.report_text().is_none());
    assert!(call(&mut s, "report", json!({"action":"open"})).is_err());
    command(&mut s, "STATUS");
    assert!(s.report_visible() && !s.is_dirty());
    assert!(call(&mut s, "report", json!({"action":"down","width":0})).is_err());
    assert!(call(&mut s, "report", json!({"action":"nonsense"})).is_err());
    assert!(s.report_visible() && !s.is_dirty());
    s.reset();
    assert!(!s.report_visible() && s.report_text().is_none());
}

#[test]
fn list_is_a_complete_visible_report_and_retains_its_status_contract() {
    let mut s = Session::default();
    for input in ["POINT", "1,1", "LINE", "2,2", "3,3", "", "LIST", "ALL"] {
        command(&mut s, input);
    }
    assert_eq!(s.report_text(), Some("1 POINT, 2 LINE\n1: layer=1 origin=(1.0000,1.0000)\n2: layer=1 start=(2.0000,2.0000) end=(3.0000,3.0000)"));
    assert_eq!(s.status(), "1 POINT, 2 LINE");
    assert!(s.report_visible());
    s.report_action(ReportAction::Close, 640, 480).unwrap();
    command(&mut s, "UNDO");
    assert_eq!(s.drawing().entities().count(), 1);
}
