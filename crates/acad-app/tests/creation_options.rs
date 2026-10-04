use acad_app::{api, Session};
use acad_model::{Entity, Point};
use serde_json::{json, Value};
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("acad-creation-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn call(s: &mut Session, method: &str, params: Value) -> Value {
    api::dispatch(
        s,
        serde_json::from_value(json!({"method":method,"params":params})).unwrap(),
        (800, 600),
    )
    .unwrap()
}
fn commands(s: &mut Session, inputs: &[&str]) {
    for input in inputs {
        call(s, "command", json!({"input":input}));
    }
}
fn bare(e: &Entity) -> &Entity {
    match e {
        Entity::OnLayer { entity, .. } => bare(entity),
        other => other,
    }
}

#[test]
fn api_circle_options_and_triangular_solid_share_mouse_clicks_files_and_undo() {
    let root = Scratch::new();
    let mut s = Session::default();
    commands(&mut s, &["SNAP", "1", "ORTHO", "ON", "CIRCLE", "2,3"]);
    call(&mut s, "point", json!({"x":5.1,"y":7.1}));
    assert!(
        matches!(bare(s.drawing().entities().next().unwrap()),Entity::Circle{center,radius} if *center==Point{x:2.0,y:3.0}&&*radius==5.0)
    );
    commands(&mut s, &["CIRCLE", "8,6", "D", "2", "CIRCLE", "2P"]);
    call(&mut s, "point", json!({"x":1.1,"y":1.1}));
    call(&mut s, "point", json!({"x":5.1,"y":1.1}));
    commands(&mut s, &["CIRCLE", "3P"]);
    let before_view = s.drawing().header.view;
    let vp = acad_render::Viewport::from_view(before_view.center, before_view.height, 800, 556);
    for world in [
        Point { x: 5.0, y: 3.0 },
        Point { x: 3.0, y: 5.0 },
        Point { x: 1.0, y: 3.0 },
    ] {
        let pixel = vp.to_screen(world);
        call(
            &mut s,
            "click",
            json!({"x":pixel.x,"y":pixel.y,"width":800,"height":600}),
        );
    }
    commands(&mut s, &["SOLID", "0,0", "4,0", "0,4", "", ""]);
    assert_eq!(s.drawing().entities().count(), 5);
    assert_eq!(s.drawing().header.view, before_view);
    let expected = s.drawing().items.clone();
    let dxf = call(&mut s, "drawing", json!({}))["data"].clone();
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing().entities().count(), 4);
    commands(&mut s, &["SOLID", "0,0", "4,0", "0,4", "", ""]);
    for ext in ["dwg", "DXF"] {
        let path = root.0.join(format!("creation options.{ext}"));
        call(&mut s, "save", json!({"path":path}));
        call(&mut s, "new", json!({}));
        call(&mut s, "open", json!({"path":path}));
        assert!(!s.is_dirty());
        assert_eq!(s.drawing().entities().count(), 5);
        if ext == "dwg" {
            assert_eq!(s.drawing().items, expected);
        }
        assert_eq!(call(&mut s, "drawing", json!({}))["data"], dxf);
        assert!(
            call(&mut s, "frame", json!({"width":320,"height":240}))["data"]
                .as_str()
                .unwrap()
                .len()
                > 100
        );
    }
}

#[test]
fn triangular_solid_renders_its_interior_and_fill_off_keeps_only_the_boundary() {
    let mut s = Session::default();
    commands(
        &mut s,
        &[
            "MENU", "", "FILL", "ON", "ZOOM", "C", "2,2", "6", "SOLID", "0,0", "4,0", "0,4", "", "",
        ],
    );
    let vp = acad_render::Viewport::from_view(Point { x: 2.0, y: 2.0 }, 6.0, 320, 196);
    let pixel = |world: Point| {
        let p = vp.to_screen(world);
        p.y.round() as usize * 320 + p.x.round() as usize
    };
    let interior = pixel(Point { x: 1.0, y: 1.0 });
    let exterior = pixel(Point { x: 3.0, y: 3.0 });
    let frame = s.frame(320, 240).unwrap();
    assert_ne!(frame.pixels[interior], 0);
    assert_eq!(frame.pixels[exterior], 0);
    commands(&mut s, &["FILL", "OFF"]);
    let frame = s.frame(320, 240).unwrap();
    assert_eq!(frame.pixels[interior], 0);
    assert_ne!(frame.pixels[pixel(Point { x: 2.0, y: 0.0 })], 0);
}
