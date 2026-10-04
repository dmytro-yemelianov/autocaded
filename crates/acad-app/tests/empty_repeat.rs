//! The original's empty REPEAT/ENDREP pair (written by its END after
//! reopening a whole-group erasure) opens through every native route as a
//! live group without members, draws and window-selects nothing, and is
//! saved back byte for byte (docs/native-group-persistence.md, "R6 empty
//! REPEAT groups"). Fixture provenance: acad-dwg/tests/fixtures.
use acad_app::{api, Session};
use acad_model::{Entity, Item, Point, Repeat};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const ENDED: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/empty-repeat-ended.dwg");
const NESTED: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/empty-repeat-nested-ended.dwg");
const ENDED_DXF: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/empty-repeat-ended.dxf");

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("acad-empty-repeat-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
    fn name(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn ok(s: &mut Session, input: &str) {
    assert_eq!(s.command(input), Ok(false), "{input:?}: {}", s.status());
}

fn records(bytes: &[u8]) -> &[u8] {
    let (_, meta) = acad_dwg::header::parse_header(bytes).unwrap();
    &bytes[meta.version.entity_start()..meta.entity_end as usize]
}

fn call(s: &mut Session, method: &str, params: Value) -> Value {
    api::dispatch(
        s,
        serde_json::from_value(json!({"method":method,"params":params})).unwrap(),
        (800, 600),
    )
    .unwrap()
}

fn empty(columns: u16, rows: u16, column_spacing: f64, row_spacing: f64) -> Repeat {
    Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: Vec::new(),
        columns,
        rows,
        column_spacing,
        row_spacing,
    }
}

/// The same header without any record: what the session must look like.
fn blank(dir: &Scratch, bytes: &[u8]) -> Session {
    let mut drawing = acad_dwg::parse(bytes).unwrap();
    drawing.items.clear();
    let path = dir.file("BLANK.DWG", &acad_dwg::write(&drawing).unwrap());
    Session::open(&path, &[]).unwrap()
}

/// One live group without members: no geometry, nothing drawn, one ID.
fn assert_empty_group(s: &Session, bytes: &[u8], blank: &Session) {
    assert_eq!(s.drawing().items, acad_dwg::parse(bytes).unwrap().items);
    assert!(matches!(s.drawing().items.as_slice(), [Item::Repeat(_)]));
    let state = api::state(s);
    assert_eq!(state["selectable_objects"], 1, "{state}");
    assert_eq!(
        s.frame(320, 240).unwrap().pixels,
        blank.frame(320, 240).unwrap().pixels,
        "the empty group draws nothing"
    );
}

#[test]
fn the_originals_empty_pair_opens_through_every_native_route() {
    let dir = Scratch::new("routes");
    for (label, bytes) in [("ENDED", ENDED), ("NESTED", NESTED)] {
        let blank = blank(&dir, bytes);
        // `acad DRAWING` and `acad-mcp --drawing` both use Session::open.
        let path = dir.file(&format!("{label}.DWG"), bytes);
        let s = Session::open(&path, &[]).unwrap();
        assert!(!s.is_dirty());
        assert_empty_group(&s, bytes, &blank);
        // Main Menu task 2.
        let mut menu = Session::main_menu(&[]);
        ok(&mut menu, "2");
        ok(&mut menu, &dir.name(label));
        assert!(!menu.main_menu_active());
        assert_empty_group(&menu, bytes, &blank);
        // API open.
        let mut api_session = Session::default();
        let opened = call(&mut api_session, "open", json!({"path":path}));
        assert_eq!(opened["state"]["selectable_objects"], 1, "{opened}");
        assert_empty_group(&api_session, bytes, &blank);
        // The file is only read.
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    let flat = Session::open(&dir.file("F.DWG", ENDED), &[]).unwrap();
    assert_eq!(api::state(&flat)["entities"], 0);
}

#[test]
fn the_mcp_server_opens_the_originals_empty_pair() {
    let dir = Scratch::new("mcp");
    let path = dir.file("ENDED.DWG", ENDED);
    let mut child = Command::new(env!("CARGO_BIN_EXE_acad-mcp"))
        .arg("--drawing")
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for message in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"acad_state","arguments":{}}}),
    ] {
        writeln!(stdin, "{message}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let state = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|v| v["id"] == 2)
        .unwrap()["result"]["structuredContent"]
        .clone();
    assert_eq!(state["entities"], 0, "{state}");
    assert_eq!(state["selectable_objects"], 1, "{state}");
    assert_eq!(state["path"], json!(path));
    assert_eq!(std::fs::read(&path).unwrap(), ENDED);
}

#[test]
fn windows_and_picks_never_find_the_empty_group() {
    let dir = Scratch::new("select");
    let path = dir.file("ENDED.DWG", ENDED);
    let mut s = Session::open(&path, &[]).unwrap();
    // As the original's `0 found.`: no window holds it.
    ok(&mut s, "ERASE");
    ok(&mut s, "W");
    ok(&mut s, "-100,-100");
    assert!(s.command("100,100").is_err());
    s.cancel().unwrap();
    assert_eq!(s.drawing().items, acad_dwg::parse(ENDED).unwrap().items);
    let editor = acad_cmd::Editor::new(s.drawing().clone());
    for point in [(0.0, 0.0), (1.0, 1.0), (6.0, 1.0)] {
        assert_eq!(
            editor.pick_entity_at(
                Point {
                    x: point.0,
                    y: point.1
                },
                100.0
            ),
            None
        );
    }
    assert!(!s.is_dirty());
}

#[test]
fn end_saves_the_pair_byte_for_byte_and_appends_after_it() {
    let dir = Scratch::new("end");
    for (label, bytes) in [("ENDED", ENDED), ("NESTED", NESTED)] {
        let path = dir.file(&format!("{label}.DWG"), bytes);
        let mut s = Session::open(&path, &[]).unwrap();
        assert_eq!(s.command("END"), Ok(true));
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(records(&saved), records(bytes), "{label}");
        assert_eq!(
            std::fs::read(dir.0.join(format!("{label}.BAK"))).unwrap(),
            bytes
        );
        // A second END is stable.
        let mut again = Session::open(&path, &[]).unwrap();
        assert_eq!(again.command("END"), Ok(true));
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
    // The flat pair re-saves as the original's whole file.
    assert_eq!(std::fs::read(dir.0.join("ENDED.DWG")).unwrap(), ENDED);
    // A new LINE goes after the pair (the original's order, count 3).
    let path = dir.file("ADD.DWG", ENDED);
    let mut s = Session::open(&path, &[]).unwrap();
    for input in ["LINE", "0,0", "5,5", ""] {
        ok(&mut s, input);
    }
    assert_eq!(s.command("END"), Ok(true));
    let saved = std::fs::read(&path).unwrap();
    let mut want = records(ENDED).to_vec();
    want.extend([1, 0, 1, 0]);
    for v in [0.0f64, 0.0, 5.0, 5.0] {
        want.extend(v.to_le_bytes());
    }
    assert_eq!(records(&saved), want);
    // The AC1.2 revision keeps the pair as well.
    let ac12 = acad_dwg::write_version(
        &acad_dwg::parse(ENDED).unwrap(),
        acad_dwg::header::Version::Ac12,
    )
    .unwrap();
    let path = dir.file("OLD.DWG", &ac12);
    let mut s = Session::open(&path, &[]).unwrap();
    assert_eq!(s.command("END"), Ok(true));
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(saved, ac12, "END keeps the AC1.2 revision and the pair");
}

#[test]
fn erasing_the_empty_group_by_number_is_native_and_round_trips() {
    let dir = Scratch::new("erase");
    let path = dir.file("ENDED.DWG", ENDED);
    let mut s = Session::open(&path, &[]).unwrap();
    ok(&mut s, "ERASE");
    ok(&mut s, "1");
    let erased = [Item::Erased(Entity::Repeat(empty(2, 1, 5.0, 0.0)))];
    assert_eq!(s.drawing().items, erased);
    ok(&mut s, "OOPS");
    assert_eq!(s.drawing().items, acad_dwg::parse(ENDED).unwrap().items);
    ok(&mut s, "ERASE");
    ok(&mut s, "1");
    assert_eq!(s.command("END"), Ok(true));
    let reopened = Session::open(&path, &[]).unwrap();
    assert_eq!(reopened.drawing().items, erased);
    assert_eq!(api::state(&reopened)["selectable_objects"], 0);
}

#[test]
fn main_menu_dxf_tasks_keep_the_pair_like_the_original() {
    let dir = Scratch::new("dxf");
    dir.file("D2.DWG", ENDED);
    // Task 5: the original's own DXF, up to its end-of-file marker.
    let mut s = Session::main_menu(&[]);
    ok(&mut s, "5");
    ok(&mut s, &dir.name("D2"));
    let end = ENDED_DXF.iter().position(|&b| b == 0x1a).unwrap();
    assert_eq!(
        std::fs::read(dir.0.join("D2.DXF")).unwrap(),
        ENDED_DXF[..=end]
    );
    // Task 6 of the original's DXF into a new drawing: the live pair.
    dir.file("D3.DXF", ENDED_DXF);
    let mut s = Session::main_menu(&[]);
    ok(&mut s, "6");
    ok(&mut s, &dir.name("D3"));
    let loaded = acad_dwg::parse(&std::fs::read(dir.0.join("D3.DWG")).unwrap()).unwrap();
    assert_eq!(loaded.items, [Item::Repeat(empty(2, 1, 5.0, 0.0))]);
}

/// Every editing, report and view command addressed to the empty group (by
/// number, LAST or ALL) either works or refuses with a message: no panic,
/// and the drawing always stays savable in both revisions.
#[test]
fn commands_on_the_empty_group_never_panic_and_stay_savable() {
    let dir = Scratch::new("commands");
    let wblock = dir.name("W.DWG");
    let scripts: Vec<Vec<&str>> = vec![
        vec!["MOVE", "1", "", "0,0", "1,1"],
        vec!["MOVE", "0,0", "1,1", "1"],
        vec!["COPY", "1", "", "0,0", "1,1"],
        vec!["COPY", "0,0", "1,1", "1"],
        vec!["ROTATE", "1", "", "0,0", "90"],
        vec!["SCALE", "1", "", "0,0", "2"],
        vec!["ARRAY", "1", "", "R", "2", "2", "1", "1"],
        vec!["CHANGE", "1", "", "L", "3"],
        vec!["CHANGE", "1", "", "", "3"],
        vec!["LIST", "1"],
        vec!["LIST", "ALL"],
        vec!["DBLIST"],
        vec!["STATUS"],
        vec!["ENTITYAREA", "1"],
        vec!["BREAK", "1"],
        vec!["FILLET", "1,2"],
        vec!["HATCH", "", "1", "0", "1"],
        vec!["BLOCK", "B", "0,0", "1"],
        vec!["INSERT", "B", "1,1", "1", "1", "0"],
        vec!["INSERT", "*B", "2,2", "1", "0"],
        vec!["WBLOCK", &wblock, "", "0,0", "LAST"],
        vec!["WBLOCK", &wblock, "B"],
        vec!["ZOOM", "E"],
        vec!["ZOOM", "A"],
        vec!["ERASE", "ALL"],
        vec!["OOPS"],
        vec!["UNDO"],
        vec!["UNDO"],
    ];
    for bytes in [ENDED, NESTED] {
        let path = dir.file("C.DWG", bytes);
        let mut s = Session::open(&path, &[]).unwrap();
        for script in &scripts {
            for input in script {
                let _ = s.command(input);
            }
            let _ = s.cancel();
            for version in [
                acad_dwg::header::Version::Ac140,
                acad_dwg::header::Version::Ac12,
            ] {
                let written = acad_dwg::write_version(s.drawing(), version)
                    .unwrap_or_else(|e| panic!("{script:?}: {e}"));
                // Byte-stable (COPY/ARRAY store a copied group as an
                // `Item::Entity`, which reads back as `Item::Repeat`).
                let reopened = acad_dwg::parse(&written).unwrap();
                assert_eq!(
                    acad_dwg::write_version(&reopened, version).unwrap(),
                    written,
                    "{script:?}"
                );
            }
            acad_dxf::try_write(s.drawing()).unwrap_or_else(|e| panic!("{script:?}: {e}"));
            s.frame(160, 120).unwrap();
        }
    }
}
