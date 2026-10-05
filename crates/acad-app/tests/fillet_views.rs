//! Synthetic native API/lifecycle contracts, no original nonzero-radius parity.
use acad_app::{api, Session};
use acad_model::Point;
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let p = std::env::temp_dir().join(format!("acad-fillet-{}-{label}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn commands(s: &mut Session, inputs: &[&str]) {
    for input in inputs {
        assert!(!s.command(input).unwrap());
    }
}
fn cmd(s: &mut Session, input: &str) -> serde_json::Value {
    api::dispatch(
        s,
        api::Request::Command {
            input: input.into(),
        },
        (400, 244),
    )
    .unwrap()
}
#[test]
fn api_canvas_aspect_points_pan_and_previous_use_shared_navigation() {
    let mut s = Session::default();
    for input in ["SNAP", "10", "ORTHO", "ON", "ZOOM", "W"] {
        cmd(&mut s, input);
    }
    for (x, y) in [(0.25, 0.5), (10.25, 4.5)] {
        api::dispatch(&mut s, api::Request::Point { x, y }, (400, 244)).unwrap();
    }
    assert_eq!(s.drawing().header.view.height, 5.0);
    assert_eq!(s.drawing().header.view.center, Point { x: 5.25, y: 2.5 });
    let view = s.drawing().header.view;
    cmd(&mut s, "PAN");
    cmd(&mut s, "1,2");
    assert_eq!(s.drawing().header.view, view);
    cmd(&mut s, "@2,-3");
    // PAN moves the view centre by the displacement, as the original does.
    assert_eq!(s.drawing().header.view.center, Point { x: 7.25, y: -0.5 });
    cmd(&mut s, "ZOOM");
    cmd(&mut s, "P");
    assert_eq!(s.drawing().header.view, view);
    s.set_viewport_size(400, 244).unwrap();
    s.reset();
    commands(&mut s, &["ZOOM", "L", "0,0", "6"]);
    assert_eq!(s.drawing().header.view.center, Point { x: 6.0, y: 3.0 });
}
#[test]
fn radius_api_dirty_cancel_undo_and_ac140_save_reopen_end() {
    let scratch = Scratch::new("save");
    let path = scratch.0.join("drawing.dwg");
    let mut s = Session::default();
    s.save(&path).unwrap();
    let clean = s.drawing().clone();
    for input in ["FILLET", "R"] {
        cmd(&mut s, input);
    }
    let before = std::fs::read(&path).unwrap();
    assert!(api::dispatch(
        &mut s,
        api::Request::Command { input: "-1".into() },
        (400, 244)
    )
    .is_err());
    assert!(!s.is_dirty());
    s.cancel().unwrap();
    assert_eq!(s.drawing(), &clean);
    for input in ["FILLET", "R", "2.5"] {
        cmd(&mut s, input);
    }
    assert_eq!(api::state(&s)["fillet_radius"], 2.5);
    assert!(s.is_dirty());
    assert!(api::dispatch(&mut s, api::Request::Drawing {}, (400, 244))
        .unwrap_err()
        .contains("FILLET radius"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    cmd(&mut s, "UNDO");
    assert_eq!(s.drawing(), &clean);
    assert!(!s.is_dirty());
    commands(&mut s, &["FILLET", "R", "2.5"]);
    assert!(s.command("END").unwrap());
    assert!(!s.is_dirty());
    let opened = Session::open(&path, &[]).unwrap();
    assert_eq!(opened.drawing().header.fillet_radius, 2.5);
    assert_eq!(api::state(&opened)["fillet_radius"], 2.5);
    // Reopen must use the remembered radius on a subsequent operation.
    let mut opened = opened;
    commands(
        &mut opened,
        &[
            "LINE", "-5,0", "5,0", "", "LINE", "0,-5", "0,5", "", "FILLET", "1,2",
        ],
    );
    assert_eq!(opened.drawing().items.len(), 3);
}
#[test]
fn unsupported_dxf_save_and_ac12_end_preserve_destination_attachment_dirty_undo() {
    let scratch = Scratch::new("refusal");
    let seed = Session::default();
    let legacy = scratch.0.join("legacy.dwg");
    let bytes = acad_dwg::write_version(seed.drawing(), acad_dwg::header::Version::Ac12).unwrap();
    std::fs::write(&legacy, &bytes).unwrap();
    let mut s = Session::open(&legacy, &[]).unwrap();
    let clean = s.drawing().clone();
    commands(&mut s, &["FILLET", "R", "2.5"]);
    let before = s.drawing().clone();
    let dxf = scratch.0.join("existing.dxf");
    std::fs::write(&dxf, b"preserved destination").unwrap();
    assert!(s.save(&dxf).unwrap_err().contains("FILLET radius"));
    assert_eq!(std::fs::read(&dxf).unwrap(), b"preserved destination");
    assert_eq!(s.drawing(), &before);
    assert_eq!(s.document_path(), Some(legacy.as_path()));
    assert_eq!(s.document_format(), Some("AC1.2"));
    assert!(s.is_dirty());
    assert!(s.command("END").unwrap_err().contains("FILLET radius"));
    assert_eq!(std::fs::read(&legacy).unwrap(), bytes);
    assert_eq!(s.drawing(), &before);
    assert!(s.is_dirty());
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &clean);
    assert!(!s.is_dirty());
    assert!(s.command("END").unwrap());
}
#[test]
fn wblock_preserves_radius_and_failed_export_does_not_attach_or_mutate() {
    let scratch = Scratch::new("wblock");
    let path = scratch.0.join("group.dwg");
    let mut s = Session::default();
    commands(&mut s, &["POINT", "1,2", "FILLET", "R", "1.25"]);
    let before = s.drawing().clone();
    commands(&mut s, &["WBLOCK", path.to_str().unwrap(), "*"]);
    assert_eq!(s.drawing(), &before);
    assert!(s.document_path().is_none());
    let output = Session::open(&path, &[]).unwrap();
    assert_eq!(output.drawing().header.fillet_radius, 1.25);
    assert_eq!(output.drawing().items, before.items);
    let bad = scratch.0.join("missing").join("group.dwg");
    commands(&mut s, &["WBLOCK", bad.to_str().unwrap()]);
    assert!(s.command("*").is_err());
    assert_eq!(s.drawing(), &before);
    assert!(s.document_path().is_none());
    assert!(s.is_dirty());
    s.cancel().unwrap();
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing().header.fillet_radius, 0.0);
    assert_eq!(s.drawing().items, before.items);
}

#[test]
fn unrepresentable_views_preserve_prompt_previous_dirty_and_saved_drawing() {
    let scratch = Scratch::new("representability");
    let mut s = Session::default();
    s.set_viewport_size(400, 244).unwrap();
    let original = s.drawing().header.view;
    commands(&mut s, &["ZOOM", "C", "0,0", "10"]);
    let path = scratch.0.join("baseline.dwg");
    s.save(&path).unwrap();
    let clean = s.drawing().clone();
    let bytes = std::fs::read(&path).unwrap();
    for inputs in [
        vec!["ZOOM", "C", "1e308,1e308", "1"],
        vec!["ZOOM", "C", "0,0", "1e-308"],
        vec!["ZOOM", "L", "1e308,1e308", "1"],
        vec!["ZOOM", "1e308X"],
        vec!["PAN", "@-1e308,-1e308", ""],
    ] {
        commands(&mut s, &inputs[..inputs.len() - 1]);
        let prompt = s.prompt().to_owned();
        assert!(s
            .command(inputs.last().unwrap())
            .unwrap_err()
            .contains("view must have"));
        assert_eq!(s.prompt(), prompt);
        assert_eq!(s.drawing(), &clean);
        assert!(!s.is_dirty());
        assert_eq!(s.document_path(), Some(path.as_path()));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        s.cancel().unwrap();
        commands(&mut s, &["ZOOM", "P"]);
        assert_eq!(s.drawing().header.view, original);
        commands(&mut s, &["ZOOM", "P"]);
        assert_eq!(s.drawing(), &clean);
        assert!(!s.is_dirty());
    }
    // View rejection adds no geometry undo; the earlier point creation is the
    // next real operation to undo, irrespective of intervening prompt failures.
    commands(&mut s, &["POINT", "1,2"]);
    let created = s.drawing().items.clone();
    commands(&mut s, &["ZOOM", "C", "1e308,1e308"]);
    assert!(s.command("1").is_err());
    s.cancel().unwrap();
    assert_eq!(s.drawing().items, created);
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &clean);
    assert!(!s.is_dirty());
}
#[test]
fn valid_extreme_views_render_points_and_invert_pixel_world_coordinates() {
    use acad_render::{flatten, Prim, Viewport};
    for height in [1e-300, 1e300] {
        let mut s = Session::default();
        s.set_viewport_size(400, 244).unwrap();
        commands(&mut s, &["MENU", "", "POINT", "0,0", "ZOOM", "C", "0,0"]);
        s.command(&height.to_string()).unwrap();
        let view = s.drawing().header.view;
        assert_eq!(view.height, height);
        let vp = Viewport::from_view(view.center, view.height, 400, 200);
        let center = vp.to_screen(view.center);
        assert_eq!(center, Point { x: 200.0, y: 100.0 });
        let world = Point {
            x: height / 4.0,
            y: -height / 4.0,
        };
        let screen = vp.to_screen(world);
        assert!(screen.x.is_finite() && screen.y.is_finite());
        let inverse = vp.to_world(screen);
        assert!((inverse.x - world.x).abs() <= height * 1e-12);
        assert!((inverse.y - world.y).abs() <= height * 1e-12);
        let primitives = flatten(s.drawing(), &vp);
        assert!(!primitives.is_empty());
        for prim in primitives {
            let points = match prim {
                Prim::Polyline(points)
                | Prim::FilledPolygon(points)
                | Prim::ColoredPolyline { points, .. }
                | Prim::ColoredFilledPolygon { points, .. } => points,
            };
            assert!(points.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        }
        let frame = s.frame(400, 244).unwrap();
        assert!((99..=101).any(|y| (199..=201).any(|x| frame.pixels[y * 400 + x] != 0)));
        // The public application point route remains usable after the view is
        // accepted and adds the exact world point, including tiny/huge scales.
        commands(&mut s, &["POINT"]);
        s.point(world).unwrap();
        assert_eq!(s.drawing().items.len(), 2);
        let acad_model::Item::Entity(entity) = s.drawing().items.last().unwrap() else {
            panic!()
        };
        let acad_model::Entity::OnLayer { entity, .. } = entity else {
            panic!()
        };
        assert!(matches!(entity.as_ref(),acad_model::Entity::Point {origin} if *origin==world));
    }
}
fn lit_near(frame: &acad_app::Frame, x: usize, y: usize) -> bool {
    (y - 1..=y + 1).any(|y| (x - 1..=x + 1).any(|x| frame.pixels[y * 400 + x] != 0))
}
#[test]
fn unusable_stored_views_display_a_limits_fit_and_map_clicks_through_it() {
    let scratch = Scratch::new("display");
    // Historical DXF without DWGVIEW: the parsed stored view height is 0.
    let dxf = scratch.0.join("noview.dxf");
    std::fs::write(&dxf, b"LIMITS,1\r\n0,12,0,9\r\nLINE,1\r\n1,1,11,8\r\n").unwrap();
    // A 1e-308 high view is usable on a one-pixel-high canvas only.
    let tiny = scratch.0.join("tiny.dwg");
    let mut resized = Session::default();
    resized.set_viewport_size(400, 1).unwrap();
    commands(&mut resized, &["ZOOM", "C", "0,0", "1e-308"]);
    resized.save(&tiny).unwrap();
    let cases = [
        ("no DWGVIEW", Session::open(&dxf, &[]).unwrap()),
        ("resized", resized),
        ("loaded", Session::open(&tiny, &[]).unwrap()),
    ];
    for (label, mut s) in cases {
        let clean = s.drawing().clone();
        assert!(clean.header.view.height <= 1e-308, "{label}");
        // Display-only fallback: the frame (drawing, prompt, command line)
        // is produced and nothing about the drawing or dirty state changes.
        let before = s.frame(400, 244).unwrap();
        assert!(
            before.pixels[230 * 400..].iter().any(|&p| p != 0),
            "{label}"
        );
        assert_eq!(s.drawing(), &clean, "{label}");
        assert!(!s.is_dirty(), "{label}");
        // A click maps through exactly the drawn viewport: the new point is
        // rendered under the clicked pixel. The stored view is untouched.
        commands(&mut s, &["POINT"]);
        assert!(!lit_near(&before, 40, 30), "{label}");
        s.click(40.0, 30.0, 400, 244).unwrap();
        assert_eq!(s.drawing().items.len(), clean.items.len() + 1, "{label}");
        assert_eq!(s.drawing().header.view, clean.header.view, "{label}");
        assert!(lit_near(&s.frame(400, 244).unwrap(), 40, 30), "{label}");
        // Commands still own view changes: ZOOM A stores a usable view.
        commands(&mut s, &["ZOOM", "A"]);
        let view = s.drawing().header.view;
        assert!(view.height.is_finite() && view.height > 1.0, "{label}");
        s.frame(400, 244).unwrap();
    }
}
