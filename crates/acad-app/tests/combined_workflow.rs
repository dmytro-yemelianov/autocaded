//! One drawing through the whole native editor over the in-process MCP route
//! (which wraps the shared API dispatch and Session): creation, window
//! selection edits, blocks and an external-file INSERT, DIM, HATCH styles and
//! `U`, circular ARRAY, BREAK, FILLET, motion SKETCH, LAYER OFF, UNDO chains,
//! a SCRIPT with DELAY on an injected clock, then DWG (AC1.40 and AC1.2) and
//! DXF saves with `.BAK` backups and reopen comparisons.
//!
//! Each step is a Rust contract covered in detail elsewhere; this test checks
//! that they compose in one session and one file lifecycle.
use acad_app::{api, mcp::Protocol, ScriptClock, Session};
use acad_model::{Drawing, Entity, Point};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "acad-combined-workflow-{}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct FakeClock(AtomicU64);
impl ScriptClock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::SeqCst))
    }
}

/// An initialized MCP protocol whose tools dispatch to one Session.
struct Mcp {
    protocol: Protocol,
    session: Session,
    id: u64,
}
impl Mcp {
    fn new(session: Session) -> Self {
        let mut mcp = Self {
            protocol: Protocol::default(),
            session,
            id: 0,
        };
        mcp.raw(json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"combined","version":"1"}}}));
        mcp.raw(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        mcp
    }
    fn raw(&mut self, message: Value) -> Option<Value> {
        let session = &mut self.session;
        let mut backend = |value| {
            api::dispatch(
                session,
                serde_json::from_value(value).map_err(|e| e.to_string())?,
                (800, 600),
            )
        };
        self.protocol.handle(message, &mut backend)
    }
    fn tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        self.id += 1;
        let reply = self
            .raw(json!({"jsonrpc":"2.0","id":self.id,"method":"tools/call","params":{"name":name,"arguments":arguments}}))
            .unwrap();
        let result = &reply["result"];
        if result["isError"] == true {
            Err(result["content"][0]["text"].as_str().unwrap().to_owned())
        } else {
            Ok(result["structuredContent"].clone())
        }
    }
    fn command(&mut self, input: &str) {
        self.tool("acad_command", json!({ "input": input }))
            .unwrap_or_else(|error| panic!("{input:?}: {error}"));
    }
    fn commands(&mut self, inputs: &[&str]) {
        for input in inputs {
            self.command(input);
        }
    }
    fn state(&mut self) -> Value {
        self.tool("acad_state", json!({})).unwrap()
    }
    fn prompt(&mut self) -> String {
        self.state()["prompt"].as_str().unwrap().to_owned()
    }
    fn objects(&mut self) -> usize {
        self.state()["selectable_objects"].as_u64().unwrap() as usize
    }
    /// Canonical DXF text of the current drawing.
    fn dxf(&mut self) -> String {
        self.tool("acad_drawing", json!({})).unwrap()["data"]
            .as_str()
            .unwrap()
            .to_owned()
    }
    fn drawing(&self) -> &Drawing {
        self.session.drawing()
    }
}

fn bare(mut entity: &Entity) -> &Entity {
    while let Entity::OnLayer { entity: inner, .. } = entity {
        entity = inner;
    }
    entity
}
fn live(drawing: &Drawing) -> Vec<Entity> {
    drawing
        .entities()
        .map(|entity| bare(entity).clone())
        .collect()
}
fn count(drawing: &Drawing, matches: impl Fn(&Entity) -> bool) -> usize {
    drawing.entities().filter(|e| matches(bare(e))).count()
}
fn last(drawing: &Drawing) -> Entity {
    bare(drawing.entities().last().unwrap()).clone()
}
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}

/// A separate saved drawing (AC1.40) holding a square and a circle, with its
/// insertion base at (1,1), and its bytes.
fn saved_part(dir: &Path) -> (PathBuf, Vec<u8>) {
    let mut part = Session::default();
    for input in [
        "LINE", "0,0", "2,0", "2,2", "0,2", "C", "CIRCLE", "1,1", "0.5", "BASE", "1,1",
    ] {
        part.command(input).unwrap();
    }
    let path = dir.join("PART.DWG");
    part.save(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    (path, bytes)
}

#[test]
fn one_drawing_through_every_native_workflow_saves_and_reopens() {
    let scratch = Scratch::new("all");
    let (part, part_bytes) = saved_part(&scratch.0);
    let clock = Arc::new(FakeClock::default());
    let mut session = Session::default();
    session.set_script_clock(clock.clone());
    let mut mcp = Mcp::new(session);

    // Creation: a closed LINE rectangle (1-4), a CIRCLE island (5), a
    // three-point ARC (6) and TEXT (7).
    mcp.commands(&[
        "LINE", "0,0", "4,0", "4,3", "0,3", "C", "CIRCLE", "2,1.5", "0.75", "ARC", "6,0", "7,1",
        "8,0", "TEXT", "0,5", "0.5", "0", "HELLO",
    ]);
    assert_eq!(mcp.objects(), 7);
    assert!(matches!(last(mcp.drawing()), Entity::Text { .. }));

    // MOVE the ARC by a window, then COPY the TEXT by a window.
    mcp.commands(&["MOVE", "0,0", "10,0", "W", "5.5,-1.5", "8.5,1.5", ""]);
    assert!(mcp.drawing().entities().any(|e| matches!(bare(e),
        Entity::Arc { center, radius, .. } if close(*center, p(17.0, 0.0)) && (*radius - 1.0).abs() < 1e-9)));
    mcp.commands(&["COPY", "0,0", "0,2", "W", "-0.5,4.5", "6,6", ""]);
    assert_eq!(
        count(mcp.drawing(), |e| matches!(e, Entity::Text { .. })),
        2
    );
    assert_eq!(mcp.objects(), 8);

    // ERASE the copy by a window, then OOPS restores it.
    let with_copy = live(mcp.drawing());
    mcp.commands(&["ERASE", "W", "-0.5,6.5", "6,8", ""]);
    assert_eq!(mcp.objects(), 7);
    mcp.command("OOPS");
    assert_eq!(live(mcp.drawing()), with_copy);

    // BLOCK and INSERT an in-drawing block, then INSERT the saved file.
    mcp.commands(&["LINE", "20,0", "21,0", "", "BLOCK", "TICK", "20,0", "LAST"]);
    assert!(mcp.drawing().block("TICK").is_some());
    mcp.commands(&["INSERT", "TICK", "25,5", "2", "", "45"]);
    assert!(matches!(last(mcp.drawing()),
        Entity::Insert { ref name, x_scale, rotation_deg, .. } if name == "TICK" && x_scale == 2.0 && rotation_deg == 45.0));
    mcp.commands(&["INSERT", part.to_str().unwrap(), "30,0", "", "", ""]);
    assert!(mcp.drawing().block("PART").is_some());
    assert!(matches!(last(mcp.drawing()),
        Entity::Insert { ref name, origin, .. } if name == "PART" && close(origin, p(30.0, 0.0))));

    // A DIM line under the rectangle (dimension line ends, then text point;
    // Return accepts the measured text).
    let before_dim = mcp.objects();
    mcp.commands(&["DIM", "0,-2", "4,-2", "2,-1.5", ""]);
    assert_eq!(mcp.prompt(), "Command");
    assert!(mcp.objects() > before_dim);
    let after_dim = mcp.drawing().clone();

    // HATCH with the O style by a window (completes at the second corner),
    // then U removes it entirely; a user-defined crosshatch with the I style
    // replaces it.
    mcp.commands(&["HATCH", "NET,O", "0.5", "0", "W", "-0.5,-0.5", "4.5,3.5"]);
    assert_eq!(mcp.prompt(), "Command");
    assert_eq!(
        mcp.drawing().blocks().count(),
        after_dim.blocks().count() + 1
    );
    mcp.command("U");
    assert_eq!(mcp.drawing(), &after_dim);
    mcp.commands(&[
        "HATCH",
        "U,I",
        "45",
        "0.25",
        "Y",
        "W",
        "-0.5,-0.5",
        "4.5,3.5",
    ]);
    assert_eq!(
        mcp.drawing().blocks().count(),
        after_dim.blocks().count() + 1
    );
    let hatch = mcp.drawing().blocks().last().unwrap();
    let hatched_lines = hatch
        .entities
        .iter()
        .filter(|e| matches!(bare(e), Entity::Line { .. }))
        .count();
    assert!(hatched_lines > 20, "{hatched_lines}");

    // Circular ARRAY: four items over the full circle.
    mcp.commands(&[
        "LINE", "40,0", "41,0", "", "ARRAY", "LAST", "C", "40,0", "90", "4",
    ]);
    let spokes = |d: &Drawing| {
        count(
            d,
            |e| matches!(e, Entity::Line { start, .. } if close(*start, p(40.0, 0.0))),
        )
    };
    assert_eq!(spokes(mcp.drawing()), 4);

    // BREAK the original spoke by picking it, leaving two pieces.
    // Picks consider visible geometry, so show the extents first.
    mcp.commands(&["ZOOM", "E"]);
    let before_break = mcp.objects();
    mcp.commands(&["BREAK", "40.7,0", "40.9,0"]);
    assert_eq!(mcp.objects(), before_break + 1);
    assert_eq!(spokes(mcp.drawing()), 4);
    assert!(mcp.drawing().entities().any(|e| matches!(bare(e),
        Entity::Line { start, end } if close(*start, p(40.9, 0.0)) && close(*end, p(41.0, 0.0)))));

    // FILLET two perpendicular lines with a positive radius (tangent arc).
    mcp.commands(&["LINE", "50,0", "55,0", "", "LINE", "50,1", "50,5", ""]);
    let ids = mcp.objects();
    mcp.commands(&["FILLET", "R", "1"]);
    mcp.commands(&["FILLET", &format!("{},{}", ids - 1, ids)]);
    assert!(mcp.drawing().entities().any(|e| matches!(bare(e),
        Entity::Arc { center, radius, .. } if close(*center, p(51.0, 1.0)) && *radius == 1.0)));
    assert_eq!(mcp.drawing().header.fillet_radius, 1.0);

    // SKETCH by pointer motion: one pen-down stroke recorded on exit.
    let before_sketch = mcp.objects();
    mcp.commands(&["SKETCH", "0.1"]);
    mcp.tool("acad_motion", json!({"x":100.0,"y":100.0}))
        .unwrap();
    mcp.tool("acad_click", json!({"x":100.0,"y":100.0}))
        .unwrap();
    for x in (120..=300).step_by(20) {
        mcp.tool("acad_motion", json!({"x":f64::from(x),"y":100.0}))
            .unwrap();
    }
    for y in (120..=200).step_by(20) {
        mcp.tool("acad_motion", json!({"x":300.0,"y":f64::from(y)}))
            .unwrap();
    }
    mcp.tool("acad_click", json!({"x":300.0,"y":200.0}))
        .unwrap();
    assert_eq!(mcp.state()["sketch"]["temporary_lines"], 2);
    mcp.command("X");
    assert_eq!(mcp.objects(), before_sketch + 2);
    assert_eq!(mcp.prompt(), "Command");

    // LAYER: draw on layer 5 inside the current view, then turn it OFF; the
    // frame changes.
    mcp.commands(&["LAYER", "5", "CIRCLE", "20,8", "1", "LAYER", "1"]);
    let shown = mcp.tool("acad_frame", json!({"format":"rgba"})).unwrap()["data"].clone();
    mcp.command("LAYER OFF 5");
    assert_eq!(mcp.state()["off_layers"], json!([5]));
    let hidden = mcp.tool("acad_frame", json!({"format":"rgba"})).unwrap()["data"].clone();
    assert!(shown != hidden, "LAYER OFF must change the frame");

    // UNDO chains: three further steps undo back to the same drawing, and an
    // ERASE ALL is undone in one step.
    let checkpoint = mcp.drawing().clone();
    let checkpoint_objects = mcp.objects();
    mcp.commands(&["POINT", "70,0", "POINT", "71,0", "ERASE", "LAST"]);
    assert_eq!(mcp.objects(), checkpoint_objects + 1);
    mcp.commands(&["UNDO", "UNDO", "UNDO"]);
    assert_eq!(mcp.drawing(), &checkpoint);
    mcp.commands(&["ERASE", "ALL"]);
    assert_eq!(mcp.objects(), 0);
    mcp.command("UNDO");
    assert_eq!(mcp.drawing(), &checkpoint);

    // SCRIPT with DELAY: the deadline waits on the injected clock only.
    let script = scratch.0.join("COMBO.SCR");
    std::fs::write(&script, "POINT 80,0\nDELAY 2000\nPOINT 81,0\n").unwrap();
    let points = |d: &Drawing| count(d, |e| matches!(e, Entity::Point { .. }));
    let before_script = points(mcp.drawing());
    mcp.tool("acad_script", json!({"path":script.to_str().unwrap()}))
        .unwrap();
    assert_eq!(points(mcp.drawing()), before_script + 1);
    mcp.tool("acad_script_tick", json!({})).unwrap();
    assert_eq!(points(mcp.drawing()), before_script + 1);
    let status = mcp.tool("acad_script_status", json!({})).unwrap();
    assert_eq!(status["state"], "delaying", "{status}");
    assert_eq!(status["delay_remaining_ms"], 2000, "{status}");
    clock.0.fetch_add(1999, Ordering::SeqCst);
    mcp.tool("acad_script_tick", json!({})).unwrap();
    assert_eq!(points(mcp.drawing()), before_script + 1);
    clock.0.fetch_add(1, Ordering::SeqCst);
    mcp.tool("acad_script_tick", json!({})).unwrap();
    assert_eq!(points(mcp.drawing()), before_script + 2);
    assert_eq!(
        mcp.tool("acad_script_status", json!({})).unwrap()["state"],
        "idle"
    );
    assert_eq!(mcp.prompt(), "Command");
    let finished = mcp.drawing().clone();

    // Save AC1.40 DWG and reopen it exactly.
    let dwg = scratch.0.join("COMBO.DWG");
    mcp.tool("acad_save", json!({"path":dwg})).unwrap();
    assert_eq!(&std::fs::read(&dwg).unwrap()[..6], b"AC1.40");
    assert_eq!(mcp.state()["dirty"], false);
    mcp.tool("acad_open", json!({"path":dwg})).unwrap();
    assert_eq!(mcp.drawing().items, finished.items);
    let reopened = mcp.state();
    assert_eq!(reopened["dirty"], false);
    assert_eq!(reopened["off_layers"], json!([5]));
    assert_eq!(reopened["fillet_radius"], 1.0);

    // DXF cannot store a nonzero FILLET radius: refused without side effects.
    let dxf = scratch.0.join("COMBO_X.DXF");
    let refused = mcp.tool("acad_save", json!({"path":dxf})).unwrap_err();
    assert!(refused.contains("FILLET radius"), "{refused}");
    assert!(!dxf.exists());
    mcp.commands(&["FILLET", "R", "0"]);
    let portable = mcp.drawing().clone();
    let portable_dxf = mcp.dxf();

    // DXF save twice: the second keeps the first file's bytes as .BAK.
    mcp.tool("acad_save", json!({"path":dxf})).unwrap();
    let first_dxf = std::fs::read(&dxf).unwrap();
    mcp.commands(&["POINT", "90,0"]);
    mcp.tool("acad_save", json!({"path":dxf})).unwrap();
    assert_eq!(
        std::fs::read(scratch.0.join("COMBO_X.BAK")).unwrap(),
        first_dxf
    );
    mcp.command("UNDO");
    mcp.tool("acad_save", json!({"path":dxf})).unwrap();
    mcp.tool("acad_open", json!({"path":dxf})).unwrap();
    assert_eq!(mcp.dxf(), portable_dxf);
    assert_eq!(mcp.state()["format"], "dxf");

    // AC1.2 has no DIMARROW field, so the DIM's arrow setting is refused
    // there; the dimension itself is plain LINE/SOLID/TEXT records. Without
    // that setting, END on an attached AC1.2 drawing keeps its revision and
    // backs up the previous bytes; reopening gives the same records.
    let refused = acad_dwg::write_version(&portable, acad_dwg::header::Version::Ac12)
        .unwrap_err()
        .to_string();
    assert!(refused.contains("DIMARROW"), "{refused}");
    let mut portable = portable;
    assert!(portable.header.dim_arrow.take().is_some());
    let legacy = scratch.0.join("LEGACY.DWG");
    let legacy_bytes = acad_dwg::write_version(&portable, acad_dwg::header::Version::Ac12).unwrap();
    std::fs::write(&legacy, &legacy_bytes).unwrap();
    mcp.tool("acad_open", json!({"path":legacy})).unwrap();
    assert_eq!(mcp.state()["format"], "AC1.2");
    mcp.commands(&["POINT", "91,0"]);
    let ended = mcp.drawing().clone();
    let quit = mcp.tool("acad_command", json!({"input":"END"})).unwrap();
    assert_eq!(quit["quit"], true, "{quit}");
    assert_eq!(
        std::fs::read(scratch.0.join("LEGACY.BAK")).unwrap(),
        legacy_bytes
    );
    assert_eq!(&std::fs::read(&legacy).unwrap()[..5], b"AC1.2");
    let reopened = Session::open(&legacy, &[]).unwrap();
    assert_eq!(reopened.drawing().items, ended.items);
    assert_eq!(reopened.document_format(), Some("AC1.2"));

    // The external source file was only read.
    assert_eq!(std::fs::read(&part).unwrap(), part_bytes);
}
