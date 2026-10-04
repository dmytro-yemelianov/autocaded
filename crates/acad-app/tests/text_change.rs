//! Native metric/layout/atomicity contracts using actual SHP programs. No
//! original aligned/repeated TEXT export parity is claimed.
use acad_app::{api, mcp::Protocol, Session};
use acad_model::{Entity, Item, Point};
use serde_json::{json, Value};
use std::path::PathBuf;
struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let p = std::env::temp_dir().join(format!("acad-text-{}-{label}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        std::fs::write(p.join("FIRST.SHP"),b"*0,4,FIRST\n10,2,0,0\n*65,11,A\n8,-2,0,8,8,0,2,8,4,0,0\n*66,8,B\n8,3,0,2,8,2,0,0\n*32,5,SPACE\n2,8,7,0,0\n").unwrap();
        std::fs::write(
            p.join("SECOND.SHP"),
            b"*0,4,SECOND\n10,2,0,0\n*65,8,A\n8,20,0,2,8,5,0,0\n",
        )
        .unwrap();
        std::fs::write(p.join("SHAPES.SHP"), b"*1,4,DOT\n8,1,0,0\n").unwrap();
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
        serde_json::from_value(json!({"method":method,"params":params})).unwrap(),
        (800, 600),
    )
}
fn commands(s: &mut Session, inputs: &[&str]) {
    for input in inputs {
        call(s, "command", json!({"input":input})).unwrap();
    }
}
fn bare(mut e: &Entity) -> &Entity {
    while let Entity::OnLayer { entity, .. } = e {
        e = entity;
    }
    e
}
fn text(s: &Session, index: usize) -> (Point, f64, f64, &str) {
    let Item::Entity(e) = &s.drawing().items[index] else {
        panic!()
    };
    let Entity::Text {
        origin,
        height,
        rotation_deg,
        value,
    } = bare(e)
    else {
        panic!()
    };
    (*origin, *height, *rotation_deg, value)
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} vs {b}");
}
#[test]
fn actual_variable_font_ink_alignment_spaces_repetition_and_idle_space() {
    let root = Scratch::new("ink");
    let mut s = Session::new(&[root.0.clone()]);
    commands(
        &mut s,
        &["LOAD", "FIRST", "TEXT", "C", "10,20", "2", "90", "A "],
    );
    let (p, h, a, value) = text(&s, 1);
    near(p.x, 10.0);
    near(p.y, 19.6);
    assert_eq!((h, a, value), (2.0, 90.0, "A "));
    s.type_characters(" ").unwrap();
    assert_eq!(s.prompt(), "TEXT: value");
    s.type_characters("B").unwrap();
    s.type_characters(" ").unwrap();
    assert_eq!(s.input(), "B ");
    let input = s.input().to_owned();
    commands(&mut s, &[&input]);
    let (p, h, a, value) = text(&s, 2);
    near(p.x, 13.0);
    near(p.y, 19.7);
    assert_eq!((h, a, value), (2.0, 90.0, "B "));
    commands(
        &mut s,
        &["LOAD", "SHAPES", "TEXT", "R", "5,6", "1", "30", "B"],
    );
    let (p, _, _, _) = text(&s, 4);
    near(p.x, 5.0 - 0.3 * 30f64.to_radians().cos());
    near(p.y, 6.0 - 0.15);
    commands(&mut s, &["TEXT", "A", "1,2", "@3,4", "A"]);
    let (p, h, a, _) = text(&s, 5);
    near(p.x, 1.75);
    near(p.y, 3.0);
    near(h, 6.25);
    near(a, 4f64.atan2(3.0).to_degrees());
    commands(&mut s, &["LOAD", "SECOND", "TEXT", "A", "1,2", "@3,4", "A"]);
    let (p, h, a, _) = text(&s, 7);
    assert_eq!(p, Point { x: 1.0, y: 2.0 });
    near(h, 2.5);
    near(a, 4f64.atan2(3.0).to_degrees());
    // Measured ink from rendering reaches both designated endpoints.
    let vp = acad_render::Viewport::from_view(Point { x: 0.0, y: 0.0 }, 40.0, 800, 600);
    let (libs, _) = acad_render::Libraries::load(&[root.0.clone()]);
    let selected = std::collections::BTreeSet::from([5, 7]);
    let rendered = acad_render::flatten_selected_with_libraries(s.drawing(), &vp, &libs, &selected);
    let screen_first = vp.to_screen(Point { x: 1.0, y: 2.0 });
    let screen_second = vp.to_screen(Point { x: 4.0, y: 6.0 });
    let points: Vec<_> = rendered
        .primitives
        .iter()
        .flat_map(|p| match p {
            acad_render::Prim::Polyline(points)
            | acad_render::Prim::ColoredPolyline { points, .. } => points.as_slice(),
            _ => &[],
        })
        .collect();
    assert!(points
        .iter()
        .any(|p| (p.x - screen_first.x).abs() < 1e-10 && (p.y - screen_first.y).abs() < 1e-10));
    assert!(points
        .iter()
        .any(|p| (p.x - screen_second.x).abs() < 1e-10 && (p.y - screen_second.y).abs() < 1e-10));
}
#[test]
fn font_context_includes_hidden_group_insert_loads_and_append_position() {
    let root = Scratch::new("context");
    let mut s = Session::new(&[root.0.clone()]);
    commands(
        &mut s,
        &[
            "LAYER",
            "2",
            "REPEAT",
            "LOAD",
            "FIRST",
            "POINT",
            "1,1",
            "ENDREP",
            "2",
            "1",
            "1",
            "0",
            "BLOCK",
            "FONTBLOCK",
            "0,0",
            "LAST",
            "LAYER OFF 2",
            "LAYER",
            "1",
            "INSERT",
            "FONTBLOCK",
            "0,0",
            "1",
            "1",
            "0",
            "TEXT",
            "C",
            "10,10",
            "1",
            "0",
            "B",
        ],
    );
    let index = s.drawing().items.len() - 1;
    let (p, _, _, _) = text(&s, index);
    near(p.x, 9.85);
    commands(&mut s, &["LOAD", "SECOND", "CHANGE", "LAST"]); // LAST is the TEXT; LOAD has no canonical ID.
    let before = s.drawing().clone();
    commands(&mut s, &["@1,2", "", "90"]);
    assert_eq!(s.drawing(), &before);
    // The original erases the changed record and appends the new text, so the
    // value is measured in the append context: B exists only in FIRST, and
    // the drawing ends after LOAD SECOND.
    assert!(call(&mut s, "command", json!({"input":"B"})).is_err());
    assert_eq!(s.drawing(), &before);
    commands(&mut s, &["A"]);
    assert!(matches!(s.drawing().items[index], Item::Erased(_)));
    let (p, _, angle, value) = text(&s, s.drawing().items.len() - 1);
    near(p.x, 10.85);
    near(p.y, 12.0);
    assert_eq!((angle, value), (90.0, "A"));
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &before);
}
#[test]
fn api_point_height_angle_missing_metrics_retry_and_atomic_clean_change() {
    let root = Scratch::new("api");
    let mut s = Session::new(&[root.0.clone()]);
    commands(&mut s, &["LOAD", "FIRST", "TEXT", "C"]);
    call(&mut s, "point", json!({"x":2,"y":3})).unwrap();
    call(&mut s, "point", json!({"x":0,"y":0})).unwrap();
    assert!(call(&mut s, "point", json!({"x":0,"y":0})).is_err());
    call(&mut s, "point", json!({"x":3,"y":4})).unwrap();
    call(&mut s, "point", json!({"x":2,"y":8})).unwrap();
    assert!(call(&mut s, "command", json!({"input":"unknown"})).is_err());
    assert_eq!(s.drawing().items.len(), 1);
    commands(&mut s, &["A"]);
    let (_, h, a, _) = text(&s, 1);
    assert_eq!((h, a), (5.0, 90.0));
    let path = root.0.join("clean.dwg");
    s.save(&path).unwrap();
    let original = s.drawing().clone();
    commands(&mut s, &["CHANGE", "LAST", "7,8", "", "30"]);
    assert!(!s.is_dirty());
    assert_eq!(s.drawing(), &original);
    assert!(call(&mut s, "command", json!({"input":"é"})).is_err());
    assert!(!s.is_dirty());
    call(&mut s, "cancel", json!({})).unwrap();
    assert_eq!(s.drawing(), &original);
    commands(&mut s, &["CHANGE", "LAST", "7,8", "", "30", "B"]);
    assert!(s.is_dirty());
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &original);
    assert!(!s.is_dirty());
    s.reset();
    commands(&mut s, &["TEXT", "C", "0,0", "1", "0"]);
    assert!(call(&mut s, "command", json!({"input":"`"})).is_err());
}
#[test]
fn baked_fields_roundtrip_both_dwg_revisions_and_dxf_without_reopening_history() {
    // TEXT A/C/R needs the retained TXT font (corpus/System/TXT.SHP).
    if corpus("System/TXT.SHP").is_none() {
        return;
    }
    let root = Scratch::new("roundtrip");
    let mut s = Session::new(&[root.0.clone()]);
    commands(
        &mut s,
        &[
            "TEXT", "C", "1,2", "2", "90", "AB", "TEXT", "R", "4,5", "3", "30", "A", "TEXT", "A",
            "1,2", "4,6", "AB",
        ],
    );
    for (name, bytes) in [
        (
            "old.dwg",
            acad_dwg::write_version(s.drawing(), acad_dwg::header::Version::Ac12).unwrap(),
        ),
        ("new.dwg", acad_dwg::write(s.drawing()).unwrap()),
        ("exchange.dxf", acad_dxf::try_write(s.drawing()).unwrap()),
    ] {
        let path = root.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        let mut reopened = Session::open(&path, &[]).unwrap();
        for index in 0..3 {
            let (p, h, a, v) = text(&s, index);
            let (q, k, b, w) = text(&reopened, index);
            let tolerance = if name.ends_with("dxf") {
                0.500001e-6
            } else {
                1e-10
            };
            for (original, reopened) in [(p.x, q.x), (p.y, q.y), (h, k), (a, b)] {
                assert!(
                    (original - reopened).abs() <= tolerance,
                    "{name}: {original} vs {reopened}"
                );
            }
            assert_eq!(v, w);
        }
        commands(&mut reopened, &["TEXT"]);
        reopened.cancel().unwrap();
        assert!(call(&mut reopened, "command", json!({"input":""})).is_err());
    }
}
#[test]
fn mcp_text_points_and_atomic_change_use_shared_api() {
    let root = Scratch::new("mcp");
    let mut s = Session::new(&[root.0.clone()]);
    let mut protocol = Protocol::default();
    let mut backend = |v| {
        api::dispatch(
            &mut s,
            serde_json::from_value(v).map_err(|e| e.to_string())?,
            (800, 600),
        )
    };
    protocol.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"text-test","version":"1"}}}),&mut backend);
    protocol.handle(
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        &mut backend,
    );
    let mut tool = |name, args| {
        protocol.handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":args}}),&mut backend).unwrap()
    };
    for input in ["LOAD", "FIRST", "TEXT", "R", "1,2", "1"] {
        assert_eq!(
            tool("acad_command", json!({"input":input}))["result"]["isError"],
            false
        );
    }
    assert_eq!(
        tool("acad_point", json!({"x":1,"y":3}))["result"]["isError"],
        false
    );
    assert_eq!(
        tool("acad_command", json!({"input":"A"}))["result"]["isError"],
        false
    );
    for input in ["CHANGE", "LAST", "3,4", "45"] {
        assert_eq!(
            tool("acad_command", json!({"input":input}))["result"]["isError"],
            false
        );
    }
    assert_eq!(
        tool("acad_command", json!({"input":"é"}))["result"]["isError"],
        true
    );
    assert_eq!(tool("acad_cancel", json!({}))["result"]["isError"], false);
    let (p, h, a, v) = text(&s, 1);
    near(p.x, 1.0);
    near(p.y, 1.4);
    assert_eq!((h, a, v), (1.0, 90.0, "A"));
}
#[test]
fn imported_missing_font_never_reuses_stale_txt_metrics() {
    let root = Scratch::new("missing");
    let path = root.0.join("missing.dxf");
    std::fs::write(&path, b"LOAD,1\r\nMISSING\r\n").unwrap();
    let mut s = Session::open(&path, &[root.0.clone()]).unwrap();
    let original = s.drawing().clone();
    commands(&mut s, &["TEXT", "C", "0,0", "1", "0"]);
    let error = call(&mut s, "command", json!({"input":"A"})).unwrap_err();
    assert!(error.contains("missing SHP font MISSING"));
    assert_eq!(s.drawing(), &original);
    assert!(!s.is_dirty());
    assert_eq!(s.prompt(), "TEXT: value");
    s.cancel().unwrap();
    commands(
        &mut s,
        &["LOAD", "FIRST", "TEXT", "C", "0,0", "1", "0", "A"],
    );
    assert!(s.is_dirty());
    let index = s.drawing().items.len() - 1;
    let (p, _, _, _) = text(&s, index);
    near(p.x, -0.2);
}
#[test]
fn screen_menu_go_repeats_text_and_blank_raw_macro_does_not() {
    let root = Scratch::new("go");
    let menu = root.0.join("TEXT.MNU");
    std::fs::write(&menu, b"[< GO >];\r\n[blank];\r\n").unwrap();
    let mut s = Session::new(&[root.0.clone()]);
    commands(
        &mut s,
        &[
            "MENU",
            menu.to_str().unwrap(),
            "LOAD",
            "FIRST",
            "TEXT",
            "C",
            "1,2",
            "1",
            "0",
            "A",
        ],
    );
    let original = s.drawing().clone();
    call(&mut s, "click", json!({"x":790,"y":20})).unwrap();
    assert_eq!(s.prompt(), "Command");
    assert_eq!(
        s.drawing(),
        &original,
        "blank raw menu macro does not repeat TEXT"
    );
    call(&mut s, "click", json!({"x":790,"y":2})).unwrap();
    assert_eq!(s.prompt(), "TEXT: value");
    s.set_input("B ".into());
    call(&mut s, "click", json!({"x":790,"y":2})).unwrap();
    let (p, h, a, v) = text(&s, 2);
    near(p.x, 0.85);
    near(p.y, 0.5);
    assert_eq!((h, a, v), (1.0, 0.0, "B "));
}

/// `corpus/<path>` when the retained corpus is extracted. Otherwise the
/// caller skips visibly; `AUTOCAD_REQUIRE_CORPUS=1` makes absence a failure.
fn corpus(path: &str) -> Option<std::path::PathBuf> {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(path);
    if full.exists() {
        return Some(full);
    }
    let message = format!("corpus {} absent", full.display());
    assert!(
        std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
        "{message}"
    );
    eprintln!("skipping corpus test, NOT validated: {message}");
    None
}
