//! HATCH `U` and external pattern files through the Session file layer, the
//! API and MCP (docs/native-hatch-user.md).
use acad_app::{api, mcp::Protocol, Session};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("acad hatch user {} {label}", std::process::id()));
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
fn call(s: &mut Session, method: &str, params: Value) -> Result<Value, String> {
    api::dispatch(
        s,
        serde_json::from_value(json!({"method":method,"params":params}))
            .map_err(|e| e.to_string())?,
        (800, 600),
    )
}
fn command(s: &mut Session, input: &str) -> Result<Value, String> {
    call(s, "command", json!({"input":input}))
}
fn commands(s: &mut Session, inputs: &[&str]) {
    for input in inputs {
        command(s, input).unwrap_or_else(|e| panic!("{input}: {e}"));
    }
}
fn prompt(s: &mut Session) -> String {
    s.prompt().to_owned()
}

const RAILS: &str = "*RAILS,two rails\r\n0, 0,0, 0,.5\r\n90, .25,0, .5,1, 0,-.5\r\n\x1a";
const ACAD_PAT: &str = "*shadow,from ACAD.PAT\n45, 0,0, 0,.5\n*rails,ACAD.PAT copy\n0, 0,0, 0,2\n";

/// A saved nested-square drawing in `dir`, opened so `dir` is the document
/// directory.
fn islands(dir: &Path) -> Session {
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
            command(&mut s, &input).unwrap();
        }
    }
    let path = dir.join("islands.dwg");
    call(&mut s, "save", json!({"path":path})).unwrap();
    call(&mut s, "open", json!({"path":path})).unwrap();
    assert!(!s.is_dirty());
    s
}

fn hatch_with(s: &mut Session, pattern: &str) -> Result<(), String> {
    command(s, "HATCH")?;
    command(s, pattern)?;
    commands(s, &["1", "0", "ALL"]);
    Ok(())
}

#[test]
fn session_looks_up_name_pat_then_acad_pat_beside_the_document() {
    let root = Scratch::new("lookup");
    std::fs::write(root.0.join("RAILS.PAT"), RAILS).unwrap();
    std::fs::write(root.0.join("ACAD.PAT"), ACAD_PAT).unwrap();
    std::fs::create_dir(root.0.join("SHADOW.PAT")).unwrap();
    std::fs::create_dir(root.0.join("sub")).unwrap();
    std::fs::write(
        root.0.join("sub/Custom.txt"),
        RAILS.replace("RAILS", "custom"),
    )
    .unwrap();
    let mut s = islands(&root.0);
    let before = s.drawing().clone();

    // RAILS.PAT wins over ACAD.PAT's *rails (spacing 0.5 rows, not 2).
    hatch_with(&mut s, "rails,O").unwrap();
    let from_file = s.drawing().clone();
    let mut reference = acad_cmd::Editor::new(before.clone());
    reference.submit("HATCH").unwrap();
    reference
        .submit_hatch_pattern_file("rails,O", "RAILS.PAT", RAILS.as_bytes())
        .unwrap();
    for input in ["1", "0", "ALL"] {
        reference.submit(input).unwrap();
    }
    assert_eq!(&from_file, reference.drawing());
    command(&mut s, "UNDO").unwrap();
    assert_eq!(s.drawing(), &before);

    // The SHADOW.PAT directory is skipped; ACAD.PAT supplies *shadow.
    hatch_with(&mut s, "Shadow").unwrap();
    let mut reference = acad_cmd::Editor::new(before.clone());
    reference.submit("HATCH").unwrap();
    reference
        .submit_hatch_pattern_file("Shadow", "ACAD.PAT", ACAD_PAT.as_bytes())
        .unwrap();
    for input in ["1", "0", "ALL"] {
        reference.submit(input).unwrap();
    }
    assert_eq!(s.drawing(), reference.drawing());
    command(&mut s, "UNDO").unwrap();

    // An explicit relative path and extension: the base name is the pattern.
    hatch_with(&mut s, "sub/Custom.txt").unwrap();
    assert_eq!(s.drawing().blocks().count(), 1);
    command(&mut s, "UNDO").unwrap();
    assert_eq!(s.drawing(), &before);

    // Built-in names never read a file, even when one exists.
    std::fs::write(root.0.join("LINE.PAT"), "*LINE\n0, 0,0, 0,3\n").unwrap();
    hatch_with(&mut s, "LINE").unwrap();
    let mut builtin = acad_cmd::Editor::new(before.clone());
    for input in ["HATCH", "LINE", "1", "0", "ALL"] {
        builtin.submit(input).unwrap();
    }
    assert_eq!(s.drawing(), builtin.drawing());
}

#[test]
fn missing_invalid_and_oversized_files_change_nothing() {
    let root = Scratch::new("errors");
    std::fs::write(root.0.join("BAD.PAT"), "*BAD\n0, 0,0, 0,0\n").unwrap();
    std::fs::write(root.0.join("EMPTY.PAT"), "*OTHER\n0, 0,0, 0,1\n").unwrap();
    let mut big = b"*BIG\n0, 0,0, 0,1\n".to_vec();
    big.resize(262_145, b'\n');
    std::fs::write(root.0.join("BIG.PAT"), big).unwrap();
    let mut s = islands(&root.0);
    let before = s.drawing().clone();
    for (input, expected) in [
        ("NOPE", "unknown HATCH pattern: NOPE"),
        ("BAD", "BAD.PAT line 2: delta-y must be positive"),
        ("EMPTY", "has no *EMPTY definition"),
        ("BIG", "262144-byte"),
        ("Q:NOPE", "FILES drive Q:"),
    ] {
        command(&mut s, "HATCH").unwrap();
        let error = command(&mut s, input).unwrap_err();
        assert!(error.contains(expected), "{input}: {error}");
        assert_eq!(prompt(&mut s), "Command", "{input}");
        assert_eq!(s.drawing(), &before);
        assert!(!s.is_dirty(), "{input}");
    }
    // A cancel after a file pattern loads leaves nothing behind.
    std::fs::write(root.0.join("OK.PAT"), "*OK\n0, 0,0, 0,1\n").unwrap();
    commands(&mut s, &["HATCH", "OK", "1"]);
    call(&mut s, "cancel", json!({})).unwrap();
    assert_eq!(s.drawing(), &before);
    assert!(!s.is_dirty());
}

#[test]
fn api_and_mcp_route_user_and_file_hatches_with_undo_and_files() {
    let root = Scratch::new("api");
    let pattern = root.0.join("RAILS.PAT");
    std::fs::write(&pattern, RAILS).unwrap();
    let mut s = islands(&root.0);
    let before = s.drawing().clone();

    // U through the API, including a budget failure that changes nothing.
    commands(&mut s, &["HATCH", "U,I", "0", "0.0001", "Y"]);
    assert!(command(&mut s, "ALL").is_err());
    assert_eq!(s.drawing(), &before);
    assert!(!s.is_dirty());
    call(&mut s, "cancel", json!({})).unwrap();
    let state = command(&mut s, "HATCH").unwrap();
    assert_eq!(
        state["state"]["prompt"],
        "HATCH: pattern (name,style / U / ?)"
    );
    commands(&mut s, &["U,O", "30", "0.5", "Y", "ALL"]);
    assert!(s.is_dirty());
    assert_eq!(s.drawing().blocks().count(), 1);
    command(&mut s, "UNDO").unwrap();
    assert_eq!(s.drawing().items, before.items);
    // An absolute file path, with a style suffix.
    commands(
        &mut s,
        &[
            "HATCH",
            &format!("{},I", pattern.display()),
            "2",
            "15",
            "ALL",
        ],
    );
    assert_eq!(s.drawing().blocks().count(), 1);
    let expected = s.drawing().items.clone();
    let dxf = call(&mut s, "drawing", json!({})).unwrap()["data"].clone();
    command(&mut s, "UNDO").unwrap();
    assert_eq!(s.drawing().items, before.items);
    commands(
        &mut s,
        &[
            "HATCH",
            &format!("{},I", pattern.display()),
            "2",
            "15",
            "ALL",
        ],
    );
    for ext in ["dwg", "dxf"] {
        let path = root.0.join(format!("hatched.{ext}"));
        call(&mut s, "save", json!({"path":path})).unwrap();
        call(&mut s, "new", json!({})).unwrap();
        call(&mut s, "open", json!({"path":path})).unwrap();
        if ext == "dwg" {
            assert_eq!(s.drawing().items, expected);
        }
        assert_eq!(call(&mut s, "drawing", json!({})).unwrap()["data"], dxf);
    }

    let mut protocol = Protocol::default();
    let mut session = islands(&root.0);
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
    for input in [
        "HATCH", "rails", "", "", "ALL", "UNDO", "HATCH", "u", "45", "1", "", "ALL",
    ] {
        let last = protocol
            .handle(
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"acad_command","arguments":{"input":input}}}),
                &mut backend,
            )
            .unwrap();
        assert_eq!(last["result"]["isError"], false, "{input}: {last}");
    }
    assert_eq!(session.drawing().blocks().count(), 1);
}
