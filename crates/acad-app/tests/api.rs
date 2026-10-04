use acad_app::{api, Session};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::path::PathBuf;
struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let p = std::env::temp_dir().join(format!("acad api {} {label}", std::process::id()));
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

#[test]
fn api_arc_continuation_undo_and_disk_round_trips_share_history_rules() {
    let scratch = Scratch::new("arc-history");
    let mut s = Session::default();
    for input in ["LINE", "0,0", "5,0", "", "ARC", "", "10,5"] {
        command(&mut s, input);
    }
    let before = s.drawing().clone();
    for input in ["ARC", "", "5,10", "UNDO", "LINE", "", "@0,3", ""] {
        command(&mut s, input);
    }
    let exported = call(&mut s, "drawing", json!({})).unwrap()["data"].clone();
    assert!(exported
        .as_str()
        .unwrap()
        .contains("10.000000,5.000000,10.000000,8.000000"));
    command(&mut s, "UNDO");
    assert_eq!(s.drawing(), &before);
    for ext in ["dwg", "dxf"] {
        let path = scratch.0.join(format!("arcs.{ext}"));
        let serialized = call(&mut s, "drawing", json!({})).unwrap()["data"].clone();
        call(&mut s, "save", json!({"path":path})).unwrap();
        call(&mut s, "open", json!({"path":path})).unwrap();
        assert_eq!(
            call(&mut s, "drawing", json!({})).unwrap()["data"],
            serialized
        );
        assert!(!s.is_dirty());
        command(&mut s, "ARC");
        assert!(call(&mut s, "command", json!({"input":""})).is_err());
        assert!(!s.is_dirty());
        call(&mut s, "cancel", json!({})).unwrap();
    }
    call(&mut s, "new", json!({})).unwrap();
    command(&mut s, "ARC");
    assert!(call(&mut s, "command", json!({"input":""})).is_err());
}

#[test]
fn api_screen_clicks_construct_arcs_with_snap_and_shared_frame_pixels() {
    use acad_model::{Entity, Point};
    let mut s = Session::default();
    for input in [
        "MENU", "", "ZOOM", "C", "5,5", "14", "SNAP", "1", "ORTHO", "ON", "ARC", "C",
    ] {
        command(&mut s, input);
    }
    let vp = acad_render::Viewport::from_view(Point { x: 5.0, y: 5.0 }, 14.0, 800, 556);
    for q in [
        Point { x: 0.1, y: 0.1 },
        Point { x: 5.1, y: 0.1 },
        Point { x: 0.1, y: 5.1 },
    ] {
        let q = vp.to_screen(q);
        call(
            &mut s,
            "click",
            json!({"x":q.x,"y":q.y,"width":800,"height":600}),
        )
        .unwrap();
    }
    assert!(
        matches!(s.drawing().entities().last(), Some(Entity::OnLayer{entity,..}) if matches!(entity.as_ref(),Entity::Arc{center,radius,start_deg,end_deg} if *center==Point{x:0.0,y:0.0} && *radius==5.0 && *start_deg==0.0 && *end_deg==90.0))
    );
    for input in ["ARC", "", "-5,0", "ARC", "5,0", "E", "0,5", "D"] {
        command(&mut s, input);
    }
    call(&mut s, "point", json!({"x":5.1,"y":1.1})).unwrap();
    let frame = s.frame(800, 600).unwrap().rgba();
    let raw = call(
        &mut s,
        "frame",
        json!({"width":800,"height":600,"format":"rgba"}),
    )
    .unwrap();
    assert_eq!(
        STANDARD.decode(raw["data"].as_str().unwrap()).unwrap(),
        frame
    );
    let q = vp.to_screen(Point { x: 3.0, y: 4.0 });
    let offset = 4 * ((q.y.round() as usize) * 800 + q.x.round() as usize);
    assert!(frame[offset..offset + 3].iter().any(|channel| *channel > 0));
    for input in ["ARC", "5,0", "E", "0,5", "R"] {
        command(&mut s, input);
    }
    let before = s.drawing().clone();
    assert!(call(&mut s, "point", json!({"x":5.0,"y":0.0})).is_err());
    assert_eq!(s.drawing(), &before);
}
#[test]
fn api_commands_geometry_and_files_use_one_session() {
    let scratch = Scratch::new("workflow");
    let mut s = Session::default();
    for input in ["SNAP", "0.5", "ORTHO", "ON", "LINE"] {
        command(&mut s, input);
    }
    call(&mut s, "point", json!({"x":1.1,"y":2.1})).unwrap();
    call(&mut s, "point", json!({"x":5.1,"y":3.0})).unwrap();
    command(&mut s, "");
    assert_eq!(s.drawing().entities().count(), 1);
    let dxf = call(&mut s, "drawing", json!({})).unwrap();
    assert!(dxf["data"]
        .as_str()
        .unwrap()
        .contains("1.000000,2.000000,5.000000,2.000000"));
    command(&mut s, "LIST");
    command(&mut s, "ALL");
    assert!(api::state(&s)["status"]
        .as_str()
        .unwrap()
        .contains("1 LINE"));
    let expected = s.drawing().items.clone();
    for ext in ["dwg", "DXF"] {
        let path = scratch.0.join(format!("saved drawing.{ext}"));
        call(&mut s, "save", json!({"path":path})).unwrap();
        call(&mut s, "new", json!({})).unwrap();
        assert_eq!(s.drawing().entities().count(), 0);
        call(&mut s, "open", json!({"path":path})).unwrap();
        assert_eq!(s.drawing().items, expected);
        for input in ["POINT", "9,4", "UNDO"] {
            command(&mut s, input);
        }
        assert_eq!(s.drawing().items, expected);
    }
    assert!(call(
        &mut s,
        "open",
        json!({"path":scratch.0.join("missing.dwg")})
    )
    .is_err());
    assert!(call(
        &mut s,
        "save",
        json!({"path":scratch.0.join("missing/drawing.dwg")})
    )
    .is_err());
    assert_eq!(s.drawing().items, expected);
    assert!(call(&mut s, "command", json!({"input":"UNKNOWN"})).is_err());
}
#[test]
fn frames_export_exact_shared_pixels_as_png_and_rgba() {
    let mut s = Session::default();
    for input in ["LINE", "1,1", "5,4", ""] {
        command(&mut s, input);
    }
    let frame = s.frame(640, 480).unwrap();
    let raw = call(
        &mut s,
        "frame",
        json!({"width":640,"height":480,"format":"rgba"}),
    )
    .unwrap();
    let pixels = STANDARD.decode(raw["data"].as_str().unwrap()).unwrap();
    assert_eq!(raw["stride"], 2560);
    assert_eq!(pixels.len(), 640 * 480 * 4);
    assert_eq!(pixels, frame.rgba());
    assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
    let png = call(&mut s, "frame", json!({"width":640,"height":480})).unwrap();
    let bytes = STANDARD.decode(png["data"].as_str().unwrap()).unwrap();
    let mut reader = png::Decoder::new(bytes.as_slice()).read_info().unwrap();
    let mut output = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!((info.width, info.height), (640, 480));
    assert_eq!(&output[..info.buffer_size()], pixels);
    for (w, h) in [(0, 480), (4097, 1), (4096, 4096)] {
        assert!(call(&mut s, "frame", json!({"width":w,"height":h})).is_err());
    }
    // Window composition keeps supporting wide physical clients; only the
    // transport enforces its smaller frame budget.
    assert_eq!(s.frame(5000, 60).unwrap().pixels.len(), 5000 * 60);
}

#[test]
fn grid_dots_share_exported_pixels_follow_snap_and_survive_save_open() {
    let scratch = Scratch::new("grid");
    let mut s = Session::default();
    for input in ["MENU", "", "LIMITS", "0,0", "4,4", "ZOOM", "C", "2,2", "6"] {
        command(&mut s, input);
    }
    let dots = |s: &mut Session| {
        let raw = call(
            s,
            "frame",
            json!({"width":640,"height":480,"format":"rgba"}),
        )
        .unwrap();
        let pixels = STANDARD.decode(raw["data"].as_str().unwrap()).unwrap();
        pixels
            .chunks_exact(4)
            .filter(|p| *p == [80, 80, 80, 255])
            .count()
    };
    assert_eq!(dots(&mut s), 0);
    for input in ["SNAP", "1", "SNAP", "OFF", "GRID", "0"] {
        command(&mut s, input);
    }
    assert_eq!(dots(&mut s), 25);
    assert_eq!(api::state(&s)["grid"], json!({"on":true,"spacing":0.0}));
    command(&mut s, "SNAP");
    command(&mut s, "0.5");
    assert_eq!(dots(&mut s), 81);
    for ext in ["dwg", "dxf"] {
        let path = scratch.0.join(format!("grid.{ext}"));
        call(&mut s, "save", json!({"path":path})).unwrap();
        call(&mut s, "open", json!({"path":path})).unwrap();
        assert!(!s.is_dirty());
        let before = s.drawing().clone();
        assert_eq!(dots(&mut s), 81);
        assert_eq!(s.drawing(), &before);
        assert!(!s.is_dirty());
    }
    let rgba = s.frame(640, 480).unwrap().rgba();
    let result = call(&mut s, "frame", json!({"width":640,"height":480})).unwrap();
    let png = STANDARD.decode(result["data"].as_str().unwrap()).unwrap();
    let mut reader = png::Decoder::new(png.as_slice()).read_info().unwrap();
    let mut decoded = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut decoded).unwrap();
    assert_eq!(&decoded[..info.buffer_size()], rgba);
    command(&mut s, "GRID");
    command(&mut s, "OFF");
    assert_eq!(dots(&mut s), 0);
}
#[test]
fn invalid_requests_and_menu_clicks_preserve_command_contracts() {
    let mut s = Session::default();
    let before = s.drawing().clone();
    assert!(call(&mut s, "point", json!({"x":1,"y":2})).is_err());
    assert!(call(&mut s, "click", json!({"x":-1,"y":5})).is_err());
    assert!(call(&mut s, "command", json!({"input":"LINE","typo":true})).is_err());
    assert_eq!(s.drawing(), &before);
    // The menu click needs the retained ACAD.MNU loaded at startup.
    if corpus("System/ACAD.MNU").is_none() {
        return;
    }
    command(&mut s, "LINE");
    // First menu page GO row then LINE row. Click route includes menu controls.
    call(&mut s, "cancel", json!({})).unwrap();
    call(
        &mut s,
        "click",
        json!({"x":650,"y":56,"width":800,"height":600}),
    )
    .unwrap();
    assert_eq!(s.prompt(), "LINE: first point");
    call(&mut s, "point", json!({"x":2,"y":3})).unwrap();
    let error = call(&mut s, "command", json!({"input":"invalid coordinate"})).unwrap_err();
    assert_eq!(s.status(), error);
    assert!(s.prompt().starts_with("LINE: next point"));
    call(&mut s, "cancel", json!({})).unwrap();
    assert_eq!(s.prompt(), "Command");
}

#[test]
fn menu_macro_retains_quit_effect_across_trailing_blank_input() {
    let scratch = Scratch::new("quit-menu");
    let path = scratch.0.join("QUIT.MNU");
    std::fs::write(&path, b"[< GO >];\r\n[quit]quit;y;\r\n").unwrap();
    let mut session = Session::default();
    command(&mut session, "MENU");
    command(&mut session, path.to_str().unwrap());
    let result = call(
        &mut session,
        "click",
        json!({"x":790,"y":20,"width":800,"height":600}),
    )
    .unwrap();
    assert_eq!(result["quit"], true);
}

#[test]
fn menu_final_separator_leaves_quit_confirmation_and_end_filename_pending() {
    let scratch = Scratch::new("document-menu");
    for (label, macro_text, prompt) in [
        ("quit", "quit;", "QUIT:"),
        ("end", "end;", "END: output file"),
    ] {
        let path = scratch.0.join(format!("{label}.MNU"));
        std::fs::write(&path, format!("[< GO >];\r\n[{label}]{macro_text}\r\n")).unwrap();
        let mut session = Session::default();
        command(&mut session, "MENU");
        command(&mut session, path.to_str().unwrap());
        let result = call(
            &mut session,
            "click",
            json!({"x":790,"y":20,"width":800,"height":600}),
        )
        .unwrap();
        assert_eq!(result["quit"], false);
        assert!(session.prompt().starts_with(prompt), "{}", session.prompt());
        call(&mut session, "cancel", json!({})).unwrap();
        assert_eq!(session.prompt(), "Command");
    }
}

#[test]
fn dot_hatches_render_and_survive_api_save_open_and_undo() {
    let scratch = Scratch::new("dot-hatches");
    for pattern in ["MUDST", "SACNCR"] {
        let mut s = Session::default();
        for input in ["LINE", "0,0", "1,0", "1,1", "0,1", "0,0", ""] {
            command(&mut s, input);
        }
        let before = s.drawing().clone();
        for input in ["HATCH", pattern, "1", "0", "ALL"] {
            command(&mut s, input);
        }
        let block = s.drawing().blocks().next().unwrap();
        assert!(block.entities.iter().any(|e| matches!(e,
            acad_model::Entity::OnLayer { entity, .. }
            if matches!(entity.as_ref(), acad_model::Entity::Point { .. }))));
        if pattern == "MUDST" {
            let vp = acad_render::Viewport::from_view(
                acad_model::Point { x: 0.5, y: 0.5 },
                1.2,
                240,
                240,
            );
            let pixels = acad_render::rasterize(&acad_render::flatten(s.drawing(), &vp), 240, 240);
            let rgb = |x: usize, y: usize| &pixels.data()[4 * (y * 240 + x)..4 * (y * 240 + x) + 3];
            // A real dot must be visible, while the gap preceding it stays black.
            assert!(rgb(120, 120).iter().any(|v| *v > 0));
            assert_eq!(rgb(95, 120), &[0, 0, 0]);
        }
        let frame = call(&mut s, "frame", json!({"width":640,"height":480})).unwrap();
        assert!(STANDARD
            .decode(frame["data"].as_str().unwrap())
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n"));
        let expected = s.drawing().items.clone();
        let dxf = call(&mut s, "drawing", json!({})).unwrap()["data"].clone();
        command(&mut s, "UNDO");
        assert_eq!(s.drawing(), &before);
        for input in ["HATCH", pattern, "1", "0", "ALL"] {
            command(&mut s, input);
        }
        for ext in ["dwg", "DXF"] {
            let path = scratch.0.join(format!("{pattern}.{ext}"));
            call(&mut s, "save", json!({"path":path})).unwrap();
            call(&mut s, "new", json!({})).unwrap();
            call(&mut s, "open", json!({"path":path})).unwrap();
            if ext == "dwg" {
                assert_eq!(s.drawing().items, expected);
            }
            // The historical DXF writer rounds coordinates to six decimals.
            assert_eq!(call(&mut s, "drawing", json!({})).unwrap()["data"], dxf);
        }
    }
}

#[test]
fn api_closes_lines_hatches_them_and_returns_complete_help_and_status_reports() {
    let mut s = Session::default();
    for input in ["LINE", "1,1", "5,1", "5,5", "1,5", "C"] {
        command(&mut s, input);
    }
    assert_eq!(s.prompt(), "Command");
    assert_eq!(s.drawing().entities().count(), 4);
    let before = s.drawing().clone();
    command(&mut s, "HELP");
    command(&mut s, "HATCH");
    assert!(api::state(&s)["report"]
        .as_str()
        .unwrap()
        .contains("Outermost area only"));
    command(&mut s, "STATUS");
    let state = api::state(&s);
    assert!(state["report"]
        .as_str()
        .unwrap()
        .contains("EXTENTS: 1.0000,1.0000 to 5.0000,5.0000"));
    assert_eq!(s.drawing(), &before);
    for input in ["HATCH", "MUDST", "1", "0", "ALL"] {
        command(&mut s, input);
    }
    assert_eq!(s.drawing().blocks().count(), 1);
    command(&mut s, "UNDO");
    assert_eq!(s.drawing(), &before);
    command(&mut s, "UNDO");
    assert_eq!(s.drawing().entities().count(), 3);
    command(&mut s, "HELP");
    assert!(call(&mut s, "command", json!({"input":"NO_SUCH_TOPIC"})).is_err());
    command(&mut s, "DIM");
    assert!(api::state(&s)["report"]
        .as_str()
        .unwrap()
        .contains("Set arrow size"));
    assert_eq!(s.prompt(), "Command");
}

#[test]
fn api_styled_hatches_share_the_command_route_budget_undo_and_files() {
    let scratch = Scratch::new("styled-hatches");
    let mut s = Session::default();
    for (lo, hi) in [(0, 12), (2, 10), (4, 8)] {
        for input in [
            "LINE".to_owned(),
            format!("{lo},{lo}"),
            format!("{hi},{lo}"),
            format!("{hi},{hi}"),
            format!("{lo},{hi}"),
            "C".to_owned(),
        ] {
            command(&mut s, &input);
        }
    }
    let path = scratch.0.join("islands.dwg");
    call(&mut s, "save", json!({"path":path})).unwrap();
    assert!(!s.is_dirty());
    let before = s.drawing().clone();

    // A tiny scale overflows the shared budget without mutating or dirtying.
    for input in ["HATCH", "NET,I", "0.001", "0"] {
        command(&mut s, input);
    }
    assert!(call(&mut s, "command", json!({"input":"ALL"})).is_err());
    assert_eq!(s.drawing(), &before);
    assert!(!s.is_dirty());
    call(&mut s, "cancel", json!({})).unwrap();

    for style in ["N", "O", "I"] {
        for input in ["HATCH", &format!("SACNCR,{style}"), "1", "0", "ALL"] {
            command(&mut s, input);
        }
        assert!(s.is_dirty());
        let expected = s.drawing().items.clone();
        let dxf = call(&mut s, "drawing", json!({})).unwrap()["data"].clone();
        for ext in ["dwg", "dxf"] {
            let path = scratch.0.join(format!("styled-{style}.{ext}"));
            call(&mut s, "save", json!({"path":path})).unwrap();
            call(&mut s, "new", json!({})).unwrap();
            call(&mut s, "open", json!({"path":path})).unwrap();
            if ext == "dwg" {
                assert_eq!(s.drawing().items, expected);
            }
            assert_eq!(call(&mut s, "drawing", json!({})).unwrap()["data"], dxf);
        }
        call(
            &mut s,
            "open",
            json!({"path":scratch.0.join("islands.dwg")}),
        )
        .unwrap();
    }
    for input in ["HATCH", "LINE,O", "1", "0", "ALL", "UNDO"] {
        command(&mut s, input);
    }
    assert_eq!(s.drawing().items, before.items);
}

#[test]
fn api_outermost_stroke_budget_fails_atomically_where_ignore_fits() {
    let mut s = Session::default();
    let rectangle = |s: &mut Session, lo: (f64, f64), hi: (f64, f64)| {
        for input in [
            "LINE".to_owned(),
            format!("{},{}", lo.0, lo.1),
            format!("{},{}", hi.0, lo.1),
            format!("{},{}", hi.0, hi.1),
            format!("{},{}", lo.0, hi.1),
            "C".to_owned(),
        ] {
            command(s, &input);
        }
    };
    rectangle(&mut s, (0.0, 0.0), (12.0, 12.0));
    for i in 0..10 {
        let x = 1.0 + f64::from(i);
        rectangle(&mut s, (x, 1.0), (x + 0.5, 11.0));
    }
    let path = std::env::temp_dir().join(format!("acad api {} islands.dwg", std::process::id()));
    call(&mut s, "save", json!({"path":path})).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(!s.is_dirty());
    let before = s.drawing().clone();
    // 12,000 rows pass the shared pre-sweep count; O's 11 spans per island
    // row exceed the 100,000-stroke budget inside the styled stroke path.
    for input in ["HATCH", "LINE,O", "0.008", "0"] {
        command(&mut s, input);
    }
    let error = call(&mut s, "command", json!({"input":"ALL"})).unwrap_err();
    assert!(error.contains("aggregate line limit"), "{error}");
    assert_eq!(s.drawing(), &before);
    assert!(!s.is_dirty());
    call(&mut s, "cancel", json!({})).unwrap();
    for input in ["HATCH", "LINE,I", "0.008", "0", "ALL"] {
        command(&mut s, input);
    }
    assert_eq!(s.drawing().blocks().next().unwrap().entities.len(), 12_000);
    command(&mut s, "UNDO");
    assert_eq!(s.drawing(), &before);
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
