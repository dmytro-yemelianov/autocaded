//! RB2 frame-level degradation: one whole-frame budget shared by the drawing
//! and selection-highlight passes, reported in diagnostics, `complete` and a
//! visible amber border plus command-area status (docs/native-render-budget.md).
use super::*;
use crate::presentation::BUDGET_INDICATOR;
use acad_model::{Entity, Item, Point, Repeat};
use std::time::{Duration, Instant};

fn lattice(columns: u16, rows: u16) -> Item {
    Item::Repeat(Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![Entity::Point {
            origin: Point { x: 1.0, y: 1.0 },
        }],
        columns,
        rows,
        column_spacing: 0.04,
        row_spacing: 0.04,
    })
}

fn session(items: Vec<Item>) -> Session {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,1\r\n").unwrap();
    drawing.items = items;
    Session::with_editor(acad_cmd::Editor::new(drawing), Libraries::default())
}

/// GUI responsiveness: before RB2 this frame expanded 65,535 owners of
/// 49,750 cells each (about 3.3e9 cells, hundreds of GB of primitives).
#[test]
fn pathological_drawing_frame_is_bounded_and_marked_incomplete() {
    let session = session(vec![lattice(250, 199); usize::from(u16::MAX)]);
    let started = Instant::now();
    let frame = session.frame(800, 600).unwrap();
    let elapsed = started.elapsed();
    eprintln!(
        "RB2 session frame (debug={}): {elapsed:?}",
        cfg!(debug_assertions)
    );
    assert!(elapsed < Duration::from_secs(30), "{elapsed:?}");
    assert!(!frame.complete);
    assert_eq!(frame.diagnostics.len(), 1, "{:?}", frame.diagnostics);
    assert!(frame.diagnostics[0].starts_with("Frame render budget of 1000000 work units"));
    assert!(frame.diagnostics[0].contains("65534 owner(s) not drawn"));
    // Amber border on the drawing canvas, not on the command area.
    assert_eq!(frame.pixels[0], BUDGET_INDICATOR);
    assert_eq!(frame.pixels[800 * 300 + 799], BUDGET_INDICATOR);
    let canvas = crate::command_line::drawing_height(600) as usize;
    assert_ne!(frame.pixels[800 * (canvas + 1)], BUDGET_INDICATOR);
}

#[test]
fn ordinary_frame_is_complete_without_indicator() {
    let session = session(vec![lattice(10, 10)]);
    let frame = session.frame(800, 600).unwrap();
    assert!(frame.complete);
    assert!(frame.diagnostics.is_empty());
    assert_ne!(frame.pixels[0], BUDGET_INDICATOR);
}

#[test]
fn selection_highlight_spends_the_same_frame_budget_and_is_omitted_whole() {
    // Seven 10,000-cell owners spend 910,000+ units; highlighting two more
    // would exceed the frame, so no partial highlight is drawn.
    let mut session = session(vec![lattice(100, 100); 7]);
    session.command("ERASE").unwrap();
    session.set_input("1,7".into());
    let frame = session.frame(800, 600).unwrap();
    assert!(!frame.complete);
    assert_eq!(
        frame.diagnostics,
        vec![
            "Selection highlight exceeds frame render budget of 1000000 work units; highlight omitted"
                .to_string()
        ]
    );
    assert_eq!(frame.pixels[0], BUDGET_INDICATOR);
    // A small drawing still highlights normally.
    let mut small = self::session(vec![lattice(10, 10); 2]);
    small.command("ERASE").unwrap();
    small.set_input("1,2".into());
    let frame = small.frame(800, 600).unwrap();
    assert!(frame.complete && frame.diagnostics.is_empty());
}

#[test]
fn api_and_mcp_frame_report_the_incomplete_frame() {
    let mut session = session(vec![lattice(250, 199); 3]);
    let request = serde_json::from_value(serde_json::json!({
        "method": "frame",
        "params": {"width": 320, "height": 240, "format": "rgba"}
    }))
    .unwrap();
    let frame = crate::api::dispatch(&mut session, request, (800, 600)).unwrap();
    assert_eq!(frame["complete"], false);
    assert_eq!(
        frame["diagnostics"],
        serde_json::json!([
            "Frame render budget of 1000000 work units exceeded: drawing stopped before item 2; 2 owner(s) not drawn"
        ])
    );
}

/// Review P2: a LOAD/recursion context failure truncates the drawing pass
/// without a budget stop; the frame must still say it is incomplete.
#[test]
fn context_failure_frame_is_incomplete_with_indicator() {
    let mut drawing = acad_dxf::parse(
        b"BLOCK,1\r\n0,0\r\nLOOP\r\nINSERT,1\r\n0,0,1,1,0\r\nLOOP\r\nENDBLK,1\r\nLINE,1\r\n0,0,1,1\r\nINSERT,2\r\n0,0,1,1,0\r\nLOOP\r\nLINE,1\r\n2,2,3,3\r\n",
    )
    .unwrap();
    drawing.header.off_layers.insert(2);
    let session = Session::with_editor(acad_cmd::Editor::new(drawing), Libraries::default());
    let frame = session.frame(800, 600).unwrap();
    assert!(!frame.complete);
    assert!(frame
        .diagnostics
        .iter()
        .any(|d| d.contains("block recursion limit reached")));
    assert_eq!(frame.pixels[0], BUDGET_INDICATOR);
    let request = serde_json::from_value(serde_json::json!({
        "method": "frame",
        "params": {"width": 320, "height": 240, "format": "rgba"}
    }))
    .unwrap();
    let mut session = session;
    let json = crate::api::dispatch(&mut session, request, (800, 600)).unwrap();
    assert_eq!(json["complete"], false);
}
