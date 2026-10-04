//! Star INSERT scale/rotation and multi-object CHANGE through the Session,
//! file codecs, API and MCP (docs/native-external-insert.md,
//! docs/native-change.md). Original prompt/geometry evidence lives in
//! acad-oracle/tests/insert_change.rs.
use acad_app::{api, mcp::Protocol, Session};
use acad_dwg::header::Version;
use acad_model::{Block, Drawing, Entity, Item, Point};
use acad_render::{flatten_with_libraries, Libraries, Prim, Viewport};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("acad-insert-change-{}-{label}", std::process::id()));
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
fn commands(s: &mut Session, inputs: &[&str]) {
    for input in inputs {
        assert!(!s.command(input).unwrap(), "{input}");
    }
}
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn on(entity: Entity) -> Entity {
    Entity::OnLayer {
        layer: 1,
        entity: Box::new(entity),
    }
}

/// Members exercising every placement rule that a uniform turn keeps.
fn members() -> Vec<Entity> {
    vec![
        on(Entity::Line {
            start: p(2.0, 1.0),
            end: p(4.0, 3.0),
        }),
        on(Entity::Circle {
            center: p(3.0, 1.0),
            radius: 0.5,
        }),
        on(Entity::Arc {
            center: p(2.0, 2.0),
            radius: 1.0,
            start_deg: 10.0,
            end_deg: 100.0,
        }),
        on(Entity::Insert {
            origin: p(4.0, 1.0),
            x_scale: 1.0,
            y_scale: 0.5,
            rotation_deg: 30.0,
            name: "INNER".into(),
        }),
    ]
}
fn inner() -> Block {
    Block {
        name: "INNER".into(),
        base: p(0.0, 0.0),
        entities: vec![on(Entity::Line {
            start: p(0.0, 0.0),
            end: p(1.0, 0.0),
        })],
    }
}

/// A source drawing with BASE (2,1), the members as roots and one erased root.
fn source() -> Drawing {
    let mut d = Session::default().drawing().clone();
    d.header.base = p(2.0, 1.0);
    d.items = vec![Item::Block(inner())];
    d.items.extend(members().into_iter().map(Item::Entity));
    d.items.push(Item::Erased(on(Entity::Line {
        start: p(2.0, 1.0),
        end: p(2.0, 3.0),
    })));
    d
}

fn write(dir: &Path, stem: &str, version: Option<Version>, drawing: &Drawing) -> PathBuf {
    let (ext, bytes) = match version {
        Some(version) => ("DWG", acad_dwg::write_version(drawing, version).unwrap()),
        None => ("DXF", acad_dxf::try_write(drawing).unwrap()),
    };
    let path = dir.join(format!("{stem}.{ext}"));
    std::fs::write(&path, bytes).unwrap();
    path
}

fn world(d: &Drawing) -> Vec<Vec<Point>> {
    let vp = Viewport::fit(
        &acad_model::Extents {
            xmin: -20.0,
            xmax: 20.0,
            ymin: -20.0,
            ymax: 20.0,
        },
        1000,
        1000,
    );
    flatten_with_libraries(d, &vp, &Libraries::default())
        .primitives
        .iter()
        .map(|prim| match prim {
            Prim::Polyline(points) | Prim::FilledPolygon(points) => {
                points.iter().map(|p| vp.to_world(*p)).collect()
            }
            Prim::ColoredPolyline { points, .. } | Prim::ColoredFilledPolygon { points, .. } => {
                points.iter().map(|p| vp.to_world(*p)).collect()
            }
        })
        .collect()
}

fn distance_to(point: Point, paths: &[Vec<Point>]) -> f64 {
    let mut best = f64::INFINITY;
    for path in paths {
        for pair in path.windows(2).chain(path.windows(1)) {
            let (a, b) = (pair[0], *pair.last().unwrap());
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = dx * dx + dy * dy;
            let t = if len == 0.0 {
                0.0
            } else {
                (((point.x - a.x) * dx + (point.y - a.y) * dy) / len).clamp(0.0, 1.0)
            };
            best = best.min((point.x - a.x - t * dx).hypot(point.y - a.y - t * dy));
        }
    }
    best
}

/// Two renders draw the same picture within tessellation tolerance.
fn same_picture(a: &Drawing, b: &Drawing) {
    let (a, b) = (world(a), world(b));
    assert!(!a.is_empty() && !b.is_empty());
    for (from, to) in [(&a, &b), (&b, &a)] {
        for point in from.iter().flatten() {
            let d = distance_to(*point, to);
            assert!(d < 0.01, "{point:?} is {d} from the other render");
        }
    }
}

#[test]
fn star_placement_renders_like_the_block_insert_it_explodes() {
    let root = Scratch::new("render");
    for (x, y, rotation) in [("2", "", "30"), ("-1.5", "", "200"), ("1", "-1", "0")] {
        let mut d = Session::default().drawing().clone();
        d.items = vec![
            Item::Block(inner()),
            Item::Block(Block {
                name: "PART".into(),
                base: p(2.0, 1.0),
                entities: members(),
            }),
        ];
        let path = write(&root.0, "HOST", Some(Version::Ac140), &d);
        let mut block = Session::open(&path, &[]).unwrap();
        let mut star = Session::open(&path, &[]).unwrap();
        commands(&mut block, &["INSERT", "PART", "1,2", x, y, rotation]);
        commands(&mut star, &["INSERT", "*PART", "S", x, y, rotation, "1,2"]);
        assert_eq!(star.prompt(), "Command");
        assert_eq!(star.drawing().items.len(), 6);
        same_picture(block.drawing(), star.drawing());
    }
}

#[test]
fn star_file_placement_from_every_format_round_trips_with_one_undo() {
    let root = Scratch::new("formats");
    for (label, version) in [
        ("AC12", Some(Version::Ac12)),
        ("AC140", Some(Version::Ac140)),
        ("DXF", None),
    ] {
        let path = write(&root.0, &format!("SRC{label}"), version, &source());
        let mut s = Session::default();
        commands(&mut s, &["POINT", "0,0"]);
        let before = s.drawing().clone();
        commands(
            &mut s,
            &[
                "INSERT",
                &format!("*{}", path.display()),
                "S",
                "2",
                "",
                "90",
            ],
        );
        assert_eq!(s.drawing(), &before, "{label}: staged only");
        commands(&mut s, &["10,10"]);
        let items = &s.drawing().items;
        // POINT, INNER, four placed roots, and (DWG sources, which store
        // erased records) the erased root kept erased.
        assert_eq!(
            items.len(),
            if version.is_some() { 7 } else { 6 },
            "{label}"
        );
        assert_eq!(
            matches!(items.last(), Some(Item::Erased(_))),
            version.is_some(),
            "{label}"
        );
        let Item::Entity(Entity::OnLayer { entity, .. }) = &items[2] else {
            panic!()
        };
        let Entity::Line { start, end } = entity.as_ref() else {
            panic!()
        };
        assert_eq!((*start, *end), (p(10.0, 10.0), p(6.0, 14.0)), "{label}");
        let placed = s.drawing().clone();
        for (ext, version) in [
            ("dwg", Some(Version::Ac12)),
            ("dwg", Some(Version::Ac140)),
            ("dxf", None),
        ] {
            let out = write(
                &root.0,
                &format!("HOST{label}{version:?}"),
                version,
                &placed,
            );
            assert!(out
                .extension()
                .unwrap()
                .to_string_lossy()
                .eq_ignore_ascii_case(ext));
            let reopened = Session::open(&out, &[]).unwrap();
            // DXF carries live records only; DWG keeps the erased root too.
            let erased = usize::from(version.is_none() && items.len() == 7);
            assert_eq!(
                reopened.drawing().items.len(),
                placed.items.len() - erased,
                "{label} {version:?}"
            );
            same_picture(reopened.drawing(), &placed);
        }
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &before, "{label}");
    }
}

#[test]
fn unrepresentable_star_file_placement_is_refused_without_change() {
    let root = Scratch::new("refuse");
    let path = write(&root.0, "PART", Some(Version::Ac140), &source());
    let mut s = Session::default();
    let before = s.drawing().clone();
    commands(
        &mut s,
        &["INSERT", &format!("*{}", path.display()), "S", "2", "1"],
    );
    let error = s.command("").unwrap_err();
    assert!(error.contains("CIRCLE"), "{error}");
    assert_eq!(s.drawing(), &before);
    assert!(!s.is_dirty());
    s.cancel().unwrap();
    assert_eq!(s.drawing(), &before);
}

fn change_setup() -> Vec<&'static str> {
    vec![
        "LINE", "0,0", "1,0", "", "BLOCK", "B1", "0,0", "LAST", "TEXT", "2,1", "0.5", "0", "T1",
        "INSERT", "B1", "4,1", "1", "", "", "LINE", "6,1", "7,1", "", "TEXT", "6,2", "0.5", "0",
        "T2", "CIRCLE", "8,1", "0.5",
    ]
}

/// Mixed CHANGE in reverse drawing order: LINE/CIRCLE move with the point;
/// T2 asks height, angle and text, then the INSERT angle, then T1.
const ANSWERS: [&str; 10] = ["CHANGE", "ALL", "3,5", "", "", "NEW", "45", "2", "30", ""];

/// `erased` is false for DXF, which carries live records only.
fn check_changed(d: &Drawing, label: &str, erased: bool) {
    let entities: Vec<(bool, &Entity)> = d
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Entity(Entity::OnLayer { entity, .. }) => Some((false, entity.as_ref())),
            Item::Erased(Entity::OnLayer { entity, .. }) => Some((true, entity.as_ref())),
            _ => None,
        })
        .collect();
    let texts: Vec<_> = entities
        .iter()
        .filter_map(|(erased, e)| match e {
            Entity::Text {
                origin,
                height,
                rotation_deg,
                value,
            } => Some((
                *erased,
                *origin,
                *height,
                (rotation_deg * 1e6).round() / 1e6,
                value.as_str(),
            )),
            _ => None,
        })
        .collect();
    let mut expected = vec![
        (false, p(3.0, 5.0), 2.0, 30.0, "T1"),
        (true, p(3.0, 5.0), 0.5, 0.0, "T2"),
        (false, p(3.0, 5.0), 0.5, 0.0, "NEW"),
    ];
    if !erased {
        expected.remove(1);
    }
    assert_eq!(texts, expected, "{label}");
    assert!(entities.iter().any(|(_, e)| matches!(e,
        Entity::Insert { origin, rotation_deg, .. } if *origin == p(3.0, 5.0) && (rotation_deg - 45.0).abs() < 1e-6)), "{label}");
    assert!(
        entities.iter().any(|(_, e)| matches!(e,
        Entity::Line { start, end } if *start == p(3.0, 5.0) && *end == p(7.0, 1.0))),
        "{label}"
    );
    assert!(
        entities.iter().any(|(_, e)| matches!(e,
        Entity::Circle { radius, .. } if (radius - 41f64.sqrt()).abs() < 1e-6)),
        "{label}"
    );
}

#[test]
fn multi_object_change_commits_once_and_round_trips_in_every_format() {
    let root = Scratch::new("change");
    let mut s = Session::default();
    commands(&mut s, &change_setup());
    let before = s.drawing().clone();
    commands(&mut s, &ANSWERS[..9]);
    assert_eq!(s.drawing(), &before, "staged until the last answer");
    commands(&mut s, &ANSWERS[9..]);
    check_changed(s.drawing(), "session", true);
    let changed = s.drawing().clone();
    for version in [Some(Version::Ac12), Some(Version::Ac140), None] {
        let out = write(&root.0, &format!("CHG{version:?}"), version, &changed);
        let reopened = Session::open(&out, &[]).unwrap();
        check_changed(
            reopened.drawing(),
            &format!("{version:?}"),
            version.is_some(),
        );
    }
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &before, "one UNDO restores everything");
}

#[test]
fn api_and_mcp_route_star_placement_and_multi_object_change() {
    let mut s = Session::default();
    let call = |s: &mut Session, input: &str| {
        api::dispatch(
            s,
            serde_json::from_value(json!({"method":"command","params":{"input":input}})).unwrap(),
            (800, 600),
        )
    };
    for input in change_setup() {
        call(&mut s, input).unwrap();
    }
    for input in &ANSWERS[..2] {
        call(&mut s, input).unwrap();
    }
    let state = call(&mut s, "3,5").unwrap();
    assert_eq!(
        state["state"]["prompt"], "CHANGE TEXT: new height (Enter keeps current)",
        "{state}"
    );
    for input in &ANSWERS[3..6] {
        call(&mut s, input).unwrap();
    }
    let state = call(&mut s, ANSWERS[6]).unwrap();
    assert_eq!(
        state["state"]["prompt"],
        "CHANGE TEXT: new height (Enter keeps current)"
    );
    for input in &ANSWERS[7..] {
        call(&mut s, input).unwrap();
    }
    check_changed(s.drawing(), "api", true);
    for input in ["INSERT", "*B1", "S", "2", "", "90"] {
        call(&mut s, input).unwrap();
    }
    let before = s.drawing().items.len();
    call(&mut s, "0,0").unwrap();
    assert_eq!(s.drawing().items.len(), before + 1);

    let mut protocol = Protocol::default();
    let mut session = Session::default();
    let mut backend = |value| {
        api::dispatch(
            &mut session,
            serde_json::from_value(value).map_err(|e| e.to_string())?,
            (640, 480),
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
    let mut last = Value::Null;
    let mut inputs = change_setup();
    inputs.extend(ANSWERS);
    inputs.extend(["INSERT", "*B1", "S", "2", "", "90", "0,0"]);
    for input in inputs {
        last = protocol
            .handle(
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"acad_command","arguments":{"input":input}}}),
                &mut backend,
            )
            .unwrap();
        assert_eq!(last["result"]["isError"], false, "{input}: {last}");
    }
    assert_eq!(
        last["result"]["structuredContent"]["state"]["prompt"], "Command",
        "{last}"
    );
    check_changed(session.drawing(), "mcp", true);
}
