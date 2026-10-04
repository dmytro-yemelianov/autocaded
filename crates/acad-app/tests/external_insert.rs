//! External drawing INSERT through the Session file layer, API and MCP
//! (docs/native-external-insert.md).
use acad_app::{api, mcp::Protocol, Session};
use acad_dwg::header::Version;
use acad_model::{Block, Drawing, Entity, Item, Point};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "acad-external-insert-{}-{label}",
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
fn commands(s: &mut Session, inputs: &[&str]) {
    for input in inputs {
        assert!(!s.command(input).unwrap(), "{input}");
    }
}
fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn on(layer: u8, entity: Entity) -> Entity {
    Entity::OnLayer {
        layer,
        entity: Box::new(entity),
    }
}

/// BASE (2,1); NUT nests INNER; layer 7 is OFF and coloured 3; one unused
/// definition must not be imported.
fn source() -> Drawing {
    let mut d = Session::default().drawing().clone();
    d.header.base = p(2.0, 1.0);
    d.header.layers.insert(7, 3);
    d.header.off_layers.insert(7);
    d.items = vec![
        Item::Block(Block {
            name: "INNER".into(),
            base: p(0.0, 0.0),
            entities: vec![on(
                1,
                Entity::Circle {
                    center: p(0.0, 0.0),
                    radius: 0.5,
                },
            )],
        }),
        Item::Block(Block {
            name: "NUT".into(),
            base: p(0.0, 0.0),
            entities: vec![on(
                1,
                Entity::Insert {
                    origin: p(1.0, 0.0),
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotation_deg: 0.0,
                    name: "INNER".into(),
                },
            )],
        }),
        Item::Block(Block {
            name: "UNUSED".into(),
            base: p(0.0, 0.0),
            entities: vec![on(
                1,
                Entity::Point {
                    origin: p(0.0, 0.0),
                },
            )],
        }),
        Item::Entity(on(
            7,
            Entity::Line {
                start: p(2.0, 1.0),
                end: p(4.0, 3.0),
            },
        )),
        Item::Entity(on(
            1,
            Entity::Insert {
                origin: p(3.0, 1.0),
                x_scale: 1.0,
                y_scale: 1.0,
                rotation_deg: 0.0,
                name: "NUT".into(),
            },
        )),
    ];
    d
}
#[derive(Clone, Copy, Debug)]
enum Kind {
    Ac12,
    Ac140,
    Dxf,
}
fn write_source(dir: &Path, stem: &str, kind: Kind) -> PathBuf {
    let drawing = source();
    let (ext, bytes) = match kind {
        Kind::Ac12 => (
            "DWG",
            acad_dwg::write_version(&drawing, Version::Ac12).unwrap(),
        ),
        Kind::Ac140 => (
            "DWG",
            acad_dwg::write_version(&drawing, Version::Ac140).unwrap(),
        ),
        Kind::Dxf => ("DXF", acad_dxf::try_write(&drawing).unwrap()),
    };
    let path = dir.join(format!("{stem}.{ext}"));
    std::fs::write(&path, bytes).unwrap();
    path
}
fn names(d: &Drawing) -> Vec<String> {
    d.blocks().map(|b| b.name.clone()).collect()
}
fn live(d: &Drawing) -> Vec<Entity> {
    d.entities().cloned().collect()
}

#[test]
fn both_dwg_revisions_and_dxf_insert_as_blocks_and_round_trip_in_both_formats() {
    let root = Scratch::new("formats");
    for kind in [Kind::Ac12, Kind::Ac140, Kind::Dxf] {
        let path = write_source(&root.0, &format!("SRC{kind:?}"), kind);
        let mut s = Session::default();
        commands(&mut s, &["POINT", "0,0"]);
        let before = s.drawing().clone();
        commands(&mut s, &["INSERT", path.to_str().unwrap()]);
        assert_eq!(s.prompt(), "INSERT: insertion point", "{kind:?}");
        assert_eq!(s.drawing(), &before, "{kind:?}: staged only");
        commands(&mut s, &["10,10", "-1", "2", "90"]);
        let name = format!("SRC{kind:?}").to_ascii_uppercase();
        assert_eq!(
            names(s.drawing()),
            ["INNER", "NUT", name.as_str()],
            "{kind:?}"
        );
        let block = s.drawing().block(&name).unwrap();
        assert_eq!(block.base, p(2.0, 1.0));
        assert_eq!(block.entities.len(), 2);
        assert_eq!(s.drawing().header.layers.get(&7), Some(&3));
        assert!(s.drawing().header.off_layers.contains(&7));
        let inserted = s.drawing().clone();
        for ext in ["dwg", "dxf"] {
            let out = root.0.join(format!("host-{kind:?}.{ext}"));
            s.save(&out).unwrap();
            let reopened = Session::open(&out, &[]).unwrap();
            assert_eq!(
                names(reopened.drawing()),
                names(&inserted),
                "{kind:?} {ext}"
            );
            assert_eq!(
                reopened.drawing().blocks().cloned().collect::<Vec<_>>(),
                inserted.blocks().cloned().collect::<Vec<_>>(),
                "{kind:?} {ext}"
            );
            assert_eq!(live(reopened.drawing()), live(&inserted), "{kind:?} {ext}");
            assert!(reopened.drawing().header.off_layers.contains(&7));
            assert!(!reopened.is_dirty());
        }
        // One UNDO removes the definitions and the insert together.
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &before, "{kind:?}");
    }
}

#[test]
fn star_form_from_a_file_explodes_at_the_stored_base() {
    let root = Scratch::new("star");
    let path = write_source(&root.0, "PART", Kind::Ac140);
    let mut s = Session::default();
    commands(
        &mut s,
        &["INSERT", &format!("*{}", path.display()), "12,11"],
    );
    assert_eq!(s.prompt(), "Command");
    assert_eq!(names(s.drawing()), ["INNER", "NUT"]);
    assert_eq!(
        live(s.drawing()),
        vec![
            on(
                7,
                Entity::Line {
                    start: p(12.0, 11.0),
                    end: p(14.0, 13.0)
                }
            ),
            on(
                1,
                Entity::Insert {
                    origin: p(13.0, 11.0),
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotation_deg: 0.0,
                    name: "NUT".into()
                }
            ),
        ]
    );
}

#[test]
fn bare_names_resolve_beside_the_document_and_existing_blocks_win() {
    let root = Scratch::new("bare");
    write_source(&root.0, "PART", Kind::Ac12);
    let host = root.0.join("HOST.DWG");
    let mut s = Session::default();
    s.save(&host).unwrap();
    let mut s = Session::open(&host, &[]).unwrap();
    commands(&mut s, &["INSERT", "PART", "0,0", "", "", ""]);
    assert_eq!(names(s.drawing()), ["INNER", "NUT", "PART"]);
    // A second bare INSERT uses the host block; the file is not reread.
    std::fs::write(root.0.join("PART.DWG"), b"garbage").unwrap();
    commands(&mut s, &["INSERT", "part", "5,5", "", "", ""]);
    assert_eq!(names(s.drawing()), ["INNER", "NUT", "PART"]);
    // A path-qualified request for the same block name is an explicit conflict.
    let other = Scratch::new("bare-other");
    let qualified = write_source(&other.0, "PART", Kind::Ac140);
    let before = s.drawing().clone();
    commands(&mut s, &["INSERT"]);
    let error = s.command(qualified.to_str().unwrap()).unwrap_err();
    assert!(error.contains("already exists"), "{error}");
    assert_eq!(s.prompt(), "INSERT: block or file name");
    assert_eq!(s.drawing(), &before);
}

#[test]
fn drive_letters_use_the_files_mapping() {
    let root = Scratch::new("drive");
    write_source(&root.0, "GEAR", Kind::Dxf);
    std::env::set_var("AUTOCAD_DRIVE_Q", &root.0);
    let mut s = Session::default();
    commands(&mut s, &["INSERT", "Q:GEAR.DXF", "0,0", "", "", ""]);
    assert!(names(s.drawing()).contains(&"GEAR".to_owned()));
    commands(&mut s, &["INSERT"]);
    let error = s.command("R:GEAR.DXF").unwrap_err();
    assert!(error.contains("AUTOCAD_DRIVE_R"), "{error}");
}

#[test]
fn missing_unreadable_and_undecodable_files_leave_drawing_undo_and_dirty_unchanged() {
    let root = Scratch::new("failures");
    std::fs::create_dir(root.0.join("DIR.DWG")).unwrap();
    std::fs::File::create(root.0.join("HUGE.DWG"))
        .unwrap()
        .set_len(65_535 * 256 + 1)
        .unwrap();
    std::fs::write(root.0.join("BAD.DWG"), b"not a drawing at all").unwrap();
    std::fs::write(
        root.0.join("CYCLE.DXF"),
        b"BLOCK,1\r\n0,0\r\nA\r\nINSERT,1\r\n0,0,1,1,0\r\nB\r\nENDBLK,1\r\nBLOCK,1\r\n0,0\r\nB\r\nINSERT,1\r\n0,0,1,1,0\r\nA\r\nENDBLK,1\r\nINSERT,1\r\n0,0,1,1,0\r\nA\r\n",
    )
    .unwrap();
    let host = root.0.join("HOST.DWG");
    let mut s = Session::default();
    commands(&mut s, &["POINT", "1,1"]);
    s.save(&host).unwrap();
    let mut s = Session::open(&host, &[]).unwrap();
    commands(&mut s, &["POINT", "2,2"]);
    let saved = Session::open(&host, &[]).unwrap().drawing().clone();
    let before = s.drawing().clone();
    for (input, expected) in [
        ("MISSING", "not found"),
        ("DIR.DWG", "not found"),
        ("HUGE", "byte drawing file limit"),
        ("BAD", "BAD"),
        ("CYCLE.DXF", "cyclic"),
    ] {
        commands(&mut s, &["INSERT"]);
        let error = s.command(input).unwrap_err();
        assert!(error.contains(expected), "{input}: {error}");
        assert_eq!(s.status(), error);
        assert_eq!(s.prompt(), "INSERT: block or file name", "{input}");
        assert_eq!(s.drawing(), &before, "{input}");
        assert!(s.is_dirty());
        assert_eq!(names(s.drawing()), Vec::<String>::new());
        s.cancel().unwrap();
    }
    // The single remaining UNDO step is the POINT: no failure pushed history.
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &saved);
    assert!(!s.is_dirty());
    commands(&mut s, &["INSERT"]);
    assert!(s.command("MISSING").is_err());
    assert!(!s.is_dirty());
}

#[test]
fn load_resources_beside_the_source_become_available() {
    let root = Scratch::new("load");
    let lib = root.0.join("ZZINS.SHP");
    std::fs::write(&lib, b"*65,5,A\n024,2,030,1,0").unwrap();
    // Not named by any LOAD in the source: must never be merged.
    std::fs::write(root.0.join("ZZOTHER.SHP"), b"*65,5,A\n024,2,030,1,0").unwrap();
    let mut src = Session::default().drawing().clone();
    src.items = vec![
        Item::Entity(on(
            1,
            Entity::Load {
                name: "ZZINS".into(),
            },
        )),
        Item::Entity(on(
            1,
            Entity::Shape {
                origin: p(0.0, 0.0),
                height: 1.0,
                rotation_deg: 0.0,
                number: 65,
            },
        )),
    ];
    std::fs::write(
        root.0.join("FONTED.DWG"),
        acad_dwg::write_version(&src, Version::Ac140).unwrap(),
    )
    .unwrap();
    let mut s = Session::default();
    commands(&mut s, &["LOAD"]);
    assert!(s.command("ZZINS").is_err());
    s.cancel().unwrap();
    let path = root.0.join("FONTED.DWG");
    // Staged then cancelled: no library is merged.
    commands(&mut s, &["INSERT", path.to_str().unwrap(), "0,0", "1"]);
    s.cancel().unwrap();
    commands(&mut s, &["LOAD"]);
    assert!(s.command("ZZINS").is_err());
    s.cancel().unwrap();
    commands(
        &mut s,
        &["INSERT", path.to_str().unwrap(), "0,0", "", "", ""],
    );
    let block = s.drawing().block("FONTED").unwrap();
    assert_eq!(
        block.entities,
        src.items
            .iter()
            .map(|item| match item {
                Item::Entity(e) => e.clone(),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>()
    );
    commands(&mut s, &["LOAD", "ZZINS"]);
    commands(&mut s, &["LOAD"]);
    assert!(s.command("ZZOTHER").is_err());
    s.cancel().unwrap();
}

#[test]
fn corpus_gate_inserts_and_saves() {
    let gate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/Samples/ANDGATE.DWG");
    if !gate.is_file() {
        return;
    }
    let source = Session::open(&gate, &[]).unwrap().drawing().clone();
    let root = Scratch::new("corpus");
    let mut s = Session::default();
    commands(
        &mut s,
        &["INSERT", gate.to_str().unwrap(), "1,1", "", "", ""],
    );
    let block = s.drawing().block("ANDGATE").unwrap().clone();
    assert_eq!(block.base, source.header.base);
    assert_eq!(
        block.entities.len(),
        source
            .items
            .iter()
            .filter(|item| !matches!(item, Item::Block(_)))
            .count()
    );
    let out = root.0.join("GATES.DWG");
    s.save(&out).unwrap();
    assert_eq!(
        Session::open(&out, &[]).unwrap().drawing().block("ANDGATE"),
        Some(&block)
    );
}

#[test]
fn api_and_mcp_route_external_insert_through_the_session() {
    let root = Scratch::new("api");
    let path = write_source(&root.0, "VALVE", Kind::Ac12);
    let mut s = Session::default();
    let call = |s: &mut Session, input: &str| {
        api::dispatch(
            s,
            serde_json::from_value(json!({"method":"command","params":{"input":input}})).unwrap(),
            (800, 600),
        )
    };
    call(&mut s, "INSERT").unwrap();
    assert!(call(&mut s, "NOPE").is_err());
    let state = call(&mut s, path.to_str().unwrap()).unwrap();
    assert_eq!(
        state["state"]["prompt"], "INSERT: insertion point",
        "{state}"
    );
    for input in ["0,0", "", "", ""] {
        call(&mut s, input).unwrap();
    }
    assert_eq!(names(s.drawing()), ["INNER", "NUT", "VALVE"]);

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
    for input in ["INSERT", &format!("*{}", path.display()), "3,3"] {
        last = protocol
            .handle(
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"acad_command","arguments":{"input":input}}}),
                &mut backend,
            )
            .unwrap();
        assert_eq!(last["result"]["isError"], false, "{last}");
    }
    assert_eq!(
        last["result"]["structuredContent"]["state"]["blocks"], 2,
        "{last}"
    );
}

#[cfg(unix)]
#[test]
fn a_fifo_named_like_a_drawing_is_skipped_without_blocking() {
    let root = Scratch::new("fifo");
    let fifo = root.0.join("PIPE.DWG");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success());
    let (send, receive) = std::sync::mpsc::channel();
    let spec = fifo.to_str().unwrap().to_owned();
    std::thread::spawn(move || {
        let mut s = Session::default();
        commands(&mut s, &["POINT", "1,1"]);
        let before = s.drawing().clone();
        commands(&mut s, &["INSERT"]);
        let error = s.command(&spec).unwrap_err();
        let unchanged = s.drawing() == &before && s.prompt() == "INSERT: block or file name";
        s.cancel().unwrap();
        commands(&mut s, &["UNDO"]);
        send.send((error, unchanged, s.drawing().entities().count()))
            .unwrap();
    });
    let (error, unchanged, remaining) = receive
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("INSERT of a FIFO must not block");
    assert!(error.contains("not found"), "{error}");
    assert!(unchanged);
    assert_eq!(remaining, 0, "a refusal must not push UNDO history");
}
