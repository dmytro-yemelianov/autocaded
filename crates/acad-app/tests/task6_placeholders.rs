//! Drawings written by the original's Main Menu task 6 (Load DXF), with its
//! zero-filled erased placeholder records, open through every native route
//! and are saved without them (docs/native-group-persistence.md, "Task 6
//! placeholder records"). Fixture provenance: acad-dwg/tests/fixtures.
use acad_app::{api, Session};
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const RAW: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/task6-line.dwg");
const ENDED: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/task6-line-ended.dwg");
const DIMARROW: &[u8] = include_bytes!("../../acad-dwg/tests/fixtures/task6-dimarrow.dwg");

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "acad-task6-placeholders-{}-{label}",
            std::process::id()
        ));
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

/// Only the live LINE counts, selects and renders: the session matches one
/// opened from the original's own re-saved (placeholder-free) drawing, apart
/// from the nine ordinary placeholders kept as non-live erased records.
fn assert_only_the_live_line(s: &Session, ended: &Session) {
    let live: Vec<_> = s
        .drawing()
        .items
        .iter()
        .filter(|item| !matches!(item, acad_model::Item::Erased(_)))
        .cloned()
        .collect();
    assert_eq!(live, ended.drawing().items);
    assert_eq!(s.drawing().items.len(), 10);
    let state = api::state(s);
    assert_eq!(state["entities"], 1);
    assert_eq!(state["selectable_objects"], 1);
    assert_eq!(
        s.frame(320, 240).unwrap().pixels,
        ended.frame(320, 240).unwrap().pixels
    );
}

#[test]
fn the_originals_task6_drawing_opens_through_every_native_route() {
    let dir = Scratch::new("routes");
    let ended = Session::open(&dir.file("ENDED.DWG", ENDED), &[]).unwrap();
    for (label, bytes) in [("RAW", RAW), ("DIM", DIMARROW)] {
        // `acad DRAWING` and `acad-mcp --drawing` both use Session::open.
        let path = dir.file(&format!("{label}.DWG"), bytes);
        let s = Session::open(&path, &[]).unwrap();
        assert!(!s.is_dirty());
        assert_only_the_live_line(&s, &ended);
        // Main Menu task 2.
        let mut menu = Session::main_menu(&[]);
        ok(&mut menu, "2");
        ok(&mut menu, &dir.name(label));
        assert!(!menu.main_menu_active());
        assert_only_the_live_line(&menu, &ended);
        // API open.
        let mut api_session = Session::default();
        let opened = call(&mut api_session, "open", json!({"path":path}));
        assert_eq!(opened["state"]["entities"], 1, "{opened}");
        assert_only_the_live_line(&api_session, &ended);
        // The file is only read.
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn the_mcp_server_opens_the_originals_task6_drawing() {
    let dir = Scratch::new("mcp");
    let path = dir.file("RAW.DWG", RAW);
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
    assert_eq!(state["entities"], 1, "{state}");
    assert_eq!(state["selectable_objects"], 1, "{state}");
    assert_eq!(state["path"], json!(path));
    assert_eq!(std::fs::read(&path).unwrap(), RAW);
}

/// RAW's records without its structural placeholders (-6 at 0x7a, -12 at
/// 0x172, -13 at 0x188): the nine ordinary ones byte for byte, then the LINE.
fn kept() -> Vec<u8> {
    let raw = records(RAW);
    [&raw[..0x7a], &raw[0x92..0x172], &raw[0x18c..]].concat()
}

#[test]
fn end_saves_without_the_structural_placeholders() {
    let dir = Scratch::new("end");
    for (label, bytes) in [("RAW", RAW), ("DIM", DIMARROW)] {
        let path = dir.file(&format!("{label}.DWG"), bytes);
        let mut s = Session::open(&path, &[]).unwrap();
        assert_eq!(s.command("END"), Ok(true));
        let saved = std::fs::read(&path).unwrap();
        assert_eq!(records(&saved), kept(), "{label}");
        assert!(records(&saved).ends_with(records(ENDED)));
        assert_eq!(
            std::fs::read(dir.0.join(format!("{label}.BAK"))).unwrap(),
            bytes,
            "the original bytes stay in the backup"
        );
        let reopened = Session::open(&path, &[]).unwrap();
        assert_eq!(reopened.drawing().entities().count(), 1);
        // A second END is stable.
        let mut again = Session::open(&path, &[]).unwrap();
        assert_eq!(again.command("END"), Ok(true));
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
}

#[test]
fn native_task6_appends_to_the_originals_task6_drawing() {
    let dir = Scratch::new("append");
    let path = dir.file("D3.DWG", RAW);
    let mut line = acad_dwg::parse(ENDED).unwrap();
    line.items = acad_dxf::parse(b"LINE,1\r\n1,1,2,3\r\n").unwrap().items;
    dir.file("D3.DXF", &acad_dxf::write(&line));
    let mut s = Session::main_menu(&[]);
    ok(&mut s, "6");
    ok(&mut s, &dir.name("D3"));
    let text = s.main_menu_text().unwrap();
    assert!(text.contains("End of drawing interchange file."), "{text}");
    let merged = acad_dwg::parse(&std::fs::read(&path).unwrap()).unwrap();
    let lines: Vec<_> = merged.entities().collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(format!("{:?}", lines[1]).contains("x: 2.0, y: 3.0"));
    assert_eq!(std::fs::read(dir.0.join("D3.BAK")).unwrap(), RAW);
    assert_eq!(
        acad_dwg::header::parse_header(&std::fs::read(&path).unwrap())
            .unwrap()
            .1
            .entity_count,
        11,
        "nine kept ordinary placeholders and two LINEs; native task 6 adds none"
    );
}
