use acad_app::{api, Session};
use serde_json::json;

#[test]
fn shared_state_report_and_frames_expose_visibility_and_undo() {
    let mut session = Session::default();
    for input in ["POINT", "3,4"] {
        session.command(input).unwrap();
    }
    let visible = session.frame(800, 600).unwrap().rgba();
    session.command("LAYER OFF 1").unwrap();
    let hidden = session.frame(800, 600).unwrap().rgba();
    assert_ne!(visible, hidden);
    let state = api::state(&session);
    assert_eq!(state["current_layer"], 1);
    assert_eq!(state["off_layers"], json!([1]));
    assert_eq!(state["layers"]["1"], 15);
    assert_eq!(state["entities"], 1);
    session.command("LAYER ?").unwrap();
    assert!(session
        .report_text()
        .unwrap()
        .contains("1  15  OFF  current"));
    assert!(api::state(&session)["report_view"]["visible"]
        .as_bool()
        .unwrap());
    session.command("UNDO").unwrap();
    assert_eq!(api::state(&session)["off_layers"], json!([]));
    assert!(session.drawing().header.layer_is_visible(1));
}
