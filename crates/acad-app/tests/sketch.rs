//! SKETCH through the shared API and MCP routes (docs/native-sketch.md):
//! motion sampling, temporary strokes absent from state/drawing/save until
//! recorded, frame visibility, UNDO per batch and DWG/DXF round trips.
use acad_app::{api, mcp::Protocol, Session};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let p = std::env::temp_dir().join(format!("acad sketch {} {label}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn call(s: &mut Session, method: &str, params: Value) -> Result<Value, String> {
    api::dispatch(
        s,
        serde_json::from_value(json!({"method":method,"params":params}))
            .map_err(|e| e.to_string())?,
        (800, 600),
    )
}
fn command(s: &mut Session, input: &str) -> Value {
    call(s, "command", json!({"input":input})).unwrap()
}
fn motion(s: &mut Session, x: f64, y: f64) -> Value {
    call(s, "motion", json!({"x":x,"y":y})).unwrap()
}
fn state(s: &mut Session) -> Value {
    call(s, "state", json!({})).unwrap()
}

/// Cyan temporary-stroke pixels in an RGBA frame's drawing region.
fn cyan_pixels(s: &mut Session) -> usize {
    let frame = call(s, "frame", json!({"format":"rgba"})).unwrap();
    let bytes = STANDARD.decode(frame["data"].as_str().unwrap()).unwrap();
    bytes
        .chunks_exact(4)
        .enumerate()
        .filter(|(index, p)| {
            let (x, y) = (index % 800, index / 800);
            (50..450).contains(&x)
                && (50..400).contains(&y)
                && p[0] < 40
                && p[1] == p[2]
                && p[1] >= 0x60
        })
        .count()
}

/// DWG stores doubles exactly; DXF text keeps six decimals.
fn assert_same_lines(actual: &[acad_model::Item], expected: &[acad_model::Item], ext: &str) {
    use acad_model::{Entity, Item};
    let line = |item: &Item| match item {
        Item::Entity(Entity::OnLayer { layer, entity }) => match entity.as_ref() {
            Entity::Line { start, end } => (*layer, *start, *end),
            other => panic!("{ext}: expected LINE, got {other:?}"),
        },
        other => panic!("{ext}: expected entity, got {other:?}"),
    };
    let tolerance = if ext == "dwg" { 0.0 } else { 1e-6 };
    assert_eq!(actual.len(), expected.len(), "{ext}");
    for (a, e) in actual.iter().zip(expected) {
        let ((la, sa, ea), (le, se, ee)) = (line(a), line(e));
        assert_eq!(la, le, "{ext}");
        for (p, q) in [(sa, se), (ea, ee)] {
            assert!(
                (p.x - q.x).abs() <= tolerance && (p.y - q.y).abs() <= tolerance,
                "{ext}: {p:?} vs {q:?}"
            );
        }
    }
}

/// Draw an L through pixels (100,100) → (300,100) → (300,300) with the pen
/// toggled by clicks, then stop with the pen up.
fn draw_l(s: &mut Session) {
    motion(s, 100.0, 100.0);
    call(s, "click", json!({"x":100.0,"y":100.0})).unwrap();
    for x in (120..=300).step_by(20) {
        motion(s, f64::from(x), 100.0);
    }
    for y in (120..=300).step_by(20) {
        motion(s, 300.0, f64::from(y));
    }
    call(s, "click", json!({"x":300.0,"y":300.0})).unwrap();
}

#[test]
fn api_motion_sketch_record_undo_and_round_trips() {
    let scratch = Scratch::new("round-trip");
    let mut s = Session::default();
    // Motion outside SKETCH is accepted and changes nothing.
    assert_eq!(motion(&mut s, 10.0, 10.0)["state"]["sketch"], Value::Null);
    command(&mut s, "SKETCH");
    command(&mut s, "0.1");
    let st = state(&mut s);
    assert_eq!(
        st["prompt"],
        "Sketch.  Pen eXit Quit Record Erase Connect ."
    );
    assert_eq!(st["sketch"]["pen_down"], false);
    assert!(call(&mut s, "motion", json!({"x":-1.0,"y":0.0})).is_err());
    assert!(call(&mut s, "motion", json!({"x":900.0,"y":0.0})).is_err());
    draw_l(&mut s);
    let st = state(&mut s);
    // Collinear samples merge: exactly the two legs of the L.
    assert_eq!(st["sketch"]["temporary_lines"], 2);
    assert_eq!(st["sketch"]["pen_down"], false);
    assert_eq!(st["entities"], 0);
    assert_eq!(st["dirty"], false);
    assert!(cyan_pixels(&mut s) > 100, "temporary strokes are drawn");
    // Saving now writes no temporary strokes.
    let early = scratch.0.join("early.dxf");
    call(&mut s, "save", json!({"path":early})).unwrap();
    let saved = acad_dxf::parse(&std::fs::read(&early).unwrap()).unwrap();
    assert_eq!(saved.entities().count(), 0);
    assert_eq!(state(&mut s)["sketch"]["temporary_lines"], 2);

    command(&mut s, "R");
    let st = state(&mut s);
    assert_eq!(st["status"], "2 lines recorded.");
    assert_eq!(st["entities"], 2);
    assert_eq!(st["sketch"]["temporary_lines"], 0);
    // A second batch, then Return = X.
    motion(&mut s, 300.0, 300.0);
    call(&mut s, "point", json!({"x":1.0,"y":1.0})).unwrap();
    assert_eq!(state(&mut s)["sketch"]["pen_down"], true);
    motion(&mut s, 150.0, 250.0);
    command(&mut s, "");
    let st = state(&mut s);
    assert_eq!(st["prompt"], "Command");
    assert_eq!(st["sketch"], Value::Null);
    assert_eq!(st["entities"], 3);
    assert_eq!(cyan_pixels(&mut s), 0);
    let recorded = s.drawing().clone();

    for ext in ["dwg", "dxf"] {
        let path = scratch.0.join(format!("sketch.{ext}"));
        call(&mut s, "save", json!({"path":path})).unwrap();
        call(&mut s, "open", json!({"path":path})).unwrap();
        assert_same_lines(&s.drawing().items, &recorded.items, ext);
        assert!(!s.is_dirty());
    }
    // A batch on the reopened drawing is one UNDO step back to the saved state.
    command(&mut s, "SKETCH");
    command(&mut s, "0.1");
    draw_l(&mut s);
    command(&mut s, "X");
    assert_eq!(state(&mut s)["entities"], 5);
    command(&mut s, "UNDO");
    assert_eq!(state(&mut s)["entities"], 3);
    assert!(!s.is_dirty());
}

#[test]
fn api_quit_and_cancel_discard_temporary_strokes() {
    for exit in ["Q", "cancel"] {
        let mut s = Session::default();
        command(&mut s, "SKETCH");
        command(&mut s, "0.1");
        draw_l(&mut s);
        command(&mut s, "R");
        motion(&mut s, 100.0, 300.0);
        command(&mut s, "P");
        motion(&mut s, 100.0, 200.0);
        assert_eq!(state(&mut s)["sketch"]["temporary_lines"], 1);
        if exit == "Q" {
            command(&mut s, "Q");
        } else {
            call(&mut s, "cancel", json!({})).unwrap();
        }
        let st = state(&mut s);
        assert_eq!(st["prompt"], "Command", "{exit}");
        assert_eq!(st["entities"], 2, "{exit}");
        // The R batch is one undo step; the discarded strokes never were.
        command(&mut s, "UNDO");
        assert_eq!(state(&mut s)["entities"], 0, "{exit}");
    }
}

#[test]
fn api_erase_and_unknown_controls() {
    let mut s = Session::default();
    command(&mut s, "SKETCH");
    command(&mut s, "0.1");
    draw_l(&mut s);
    command(&mut s, "E");
    motion(&mut s, 300.0, 250.0);
    let st = state(&mut s);
    assert_eq!(st["status"], "Erase:  Select end of delete.");
    assert_eq!(st["sketch"]["mode"], "erase");
    assert_eq!(st["sketch"]["erase_from"], 1);
    command(&mut s, "P");
    assert_eq!(state(&mut s)["sketch"]["temporary_lines"], 1);
    let before = state(&mut s)["sketch"].clone();
    assert!(call(&mut s, "command", json!({"input":"Z"})).is_err());
    assert_eq!(state(&mut s)["sketch"], before);
    command(&mut s, "X");
    assert_eq!(state(&mut s)["entities"], 1);
}

#[test]
fn mcp_motion_tool_reaches_the_shared_session() {
    let mut session = Session::default();
    let mut protocol = Protocol::default();
    let mut backend = |request: Value| {
        api::dispatch(
            &mut session,
            serde_json::from_value(request).map_err(|e| e.to_string())?,
            (800, 600),
        )
    };
    protocol.handle(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        &mut backend,
    );
    protocol.handle(
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        &mut backend,
    );
    let mut tool = |name: &str, arguments: Value| {
        let reply = protocol
            .handle(
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
                &mut backend,
            )
            .unwrap();
        assert_eq!(reply["result"]["isError"], false, "{name}: {reply}");
        reply["result"]["structuredContent"].clone()
    };
    tool("acad_command", json!({"input":"SKETCH"}));
    tool("acad_command", json!({"input":"0.1"}));
    tool("acad_motion", json!({"x":100,"y":100}));
    tool("acad_command", json!({"input":"P"}));
    let reply = tool("acad_motion", json!({"x":250,"y":100}));
    assert_eq!(reply["state"]["sketch"]["temporary_lines"], 1);
    let reply = tool("acad_command", json!({"input":"X"}));
    assert_eq!(reply["state"]["entities"], 1);
}
