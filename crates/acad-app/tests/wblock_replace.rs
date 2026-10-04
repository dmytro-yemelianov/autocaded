//! WBLOCK overwrite contract (docs/native-files-menu.md). The original's
//! question, its order and answer rule are measured in
//! crates/acad-oracle/tests/files_backup.rs; same-file detection, the
//! create-only race guard and host safety checks are native policy.
use acad_app::{api, Session};
use acad_model::Point;
use serde_json::json;
use std::path::{Path, PathBuf};

const QUESTION: &str = "WBLOCK: A drawing with this name already exists. Replace it? <N>";
const OPEN_QUESTION: &str = "WBLOCK: This is the open drawing's file. Replace it? <N>";
const BLOCK_NAME: &str = "WBLOCK: block name (* for entire drawing)";

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("acad-wblock-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("chflags")
            .arg("-R")
            .arg("nouchg")
            .arg(&self.0)
            .status();
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn commands(s: &mut Session, values: &[&str]) {
    for value in values {
        assert!(!s.command(value).unwrap(), "{value}");
    }
}

fn point(s: &mut Session, x: f64) {
    commands(s, &["POINT"]);
    s.point(Point { x, y: 4.0 }).unwrap();
}

/// A saved drawing at `path` holding one POINT at `x`, and its bytes.
fn saved(path: &Path, x: f64) -> Vec<u8> {
    let mut s = Session::default();
    point(&mut s, x);
    s.save(path).unwrap();
    std::fs::read(path).unwrap()
}

fn points(path: &Path) -> usize {
    Session::open(path, &[])
        .unwrap()
        .drawing()
        .entities()
        .count()
}

#[test]
fn a_new_destination_is_written_without_a_question() {
    let root = Scratch::new("new");
    let target = root.0.join("part.dwg");
    let mut s = Session::default();
    point(&mut s, 1.0);
    commands(&mut s, &["WBLOCK", target.to_str().unwrap()]);
    assert_eq!(s.prompt(), BLOCK_NAME);
    commands(&mut s, &["*"]);
    assert_eq!(points(&target), 1);
    assert_eq!(root.names(), ["part.dwg"]);
}

#[test]
fn declining_keeps_file_drawing_undo_dirty_and_input() {
    for answer in ["", "N", "no", "X", "*"] {
        let root = Scratch::new("decline");
        let target = root.0.join("part.dwg");
        let before = saved(&target, 9.0);
        let mut s = Session::default();
        point(&mut s, 1.0);
        let drawing = s.drawing().clone();
        commands(&mut s, &["WBLOCK", target.to_str().unwrap()]);
        assert_eq!(s.prompt(), QUESTION);
        commands(&mut s, &[answer]);
        assert_eq!(s.prompt(), "Command", "{answer:?}");
        assert!(s.status().contains("kept existing"), "{}", s.status());
        assert_eq!(std::fs::read(&target).unwrap(), before);
        assert_eq!(root.names(), ["part.dwg"], "no backup or staging residue");
        assert_eq!(s.drawing(), &drawing);
        assert!(s.is_dirty());
        assert!(s.document_path().is_none());
        assert_eq!(s.input(), "");
        commands(&mut s, &["UNDO"]);
        assert!(s.drawing().items.is_empty(), "undo reaches the POINT");
    }
}

#[test]
fn cancel_at_the_question_keeps_the_file() {
    let root = Scratch::new("cancel");
    let target = root.0.join("part.dwg");
    let before = saved(&target, 9.0);
    let mut s = Session::default();
    point(&mut s, 1.0);
    commands(&mut s, &["WBLOCK", target.to_str().unwrap()]);
    s.cancel().unwrap();
    assert_eq!(s.prompt(), "Command");
    assert_eq!(std::fs::read(&target).unwrap(), before);
}

#[test]
fn yes_replaces_staged_without_a_backup() {
    for answer in ["Y", "yes", "Yep"] {
        let root = Scratch::new("yes");
        let target = root.0.join("part.dwg");
        saved(&target, 9.0);
        let mut s = Session::default();
        point(&mut s, 1.0);
        point(&mut s, 2.0);
        commands(&mut s, &["WBLOCK", target.to_str().unwrap(), answer]);
        assert_eq!(s.prompt(), BLOCK_NAME);
        commands(&mut s, &["*"]);
        assert_eq!(points(&target), 2, "{answer}");
        assert_eq!(root.names(), ["part.dwg"]);
        assert!(s.document_path().is_none());
    }
}

/// The question protects the destination only until it is answered; a file
/// that appears after a no-question file name is never replaced.
#[test]
fn a_file_created_after_the_file_name_is_not_replaced() {
    let root = Scratch::new("race");
    let target = root.0.join("part.dwg");
    let mut s = Session::default();
    point(&mut s, 1.0);
    commands(&mut s, &["WBLOCK", target.to_str().unwrap()]);
    std::fs::write(&target, b"appeared").unwrap();
    let error = s.command("*").unwrap_err();
    assert!(error.contains("already exists"), "{error}");
    assert_eq!(std::fs::read(&target).unwrap(), b"appeared");
    assert_eq!(root.names(), ["part.dwg"]);
    assert_eq!(s.prompt(), "Command");
}

#[test]
fn a_directory_destination_fails_at_the_file_name() {
    let root = Scratch::new("directory");
    let target = root.0.join("part.dwg");
    std::fs::create_dir(&target).unwrap();
    let mut s = Session::default();
    point(&mut s, 1.0);
    commands(&mut s, &["WBLOCK"]);
    let error = s.command(target.to_str().unwrap()).unwrap_err();
    assert!(error.contains("not a regular file"), "{error}");
    assert_eq!(s.prompt(), "Command");
    assert!(target.is_dir());
}

#[cfg(unix)]
#[test]
fn a_dangling_symlink_counts_as_existing_and_a_link_is_followed() {
    let root = Scratch::new("links");
    let dangling = root.0.join("dangling.dwg");
    std::os::unix::fs::symlink(root.0.join("missing.dwg"), &dangling).unwrap();
    let mut s = Session::default();
    point(&mut s, 1.0);
    commands(&mut s, &["WBLOCK", dangling.to_str().unwrap()]);
    assert_eq!(s.prompt(), QUESTION);
    commands(&mut s, &["N"]);
    assert!(std::fs::symlink_metadata(&dangling).unwrap().is_symlink());
    assert!(!root.0.join("missing.dwg").exists());

    let real = root.0.join("real.dwg");
    saved(&real, 9.0);
    let link = root.0.join("link.dwg");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    commands(&mut s, &["WBLOCK", link.to_str().unwrap(), "Y", "*"]);
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(points(&real), 1);
}

#[cfg(unix)]
#[test]
fn a_read_only_destination_fails_after_yes_and_keeps_its_bytes() {
    use std::os::unix::fs::PermissionsExt;
    let root = Scratch::new("readonly");
    let target = root.0.join("part.dwg");
    let before = saved(&target, 9.0);
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o444)).unwrap();
    let mut s = Session::default();
    point(&mut s, 1.0);
    commands(&mut s, &["WBLOCK", target.to_str().unwrap(), "Y"]);
    let error = s.command("*").unwrap_err();
    assert!(error.contains("read-only"), "{error}");
    assert_eq!(std::fs::read(&target).unwrap(), before);
    assert_eq!(root.names(), ["part.dwg"]);
}

#[cfg(target_os = "macos")]
#[test]
fn a_locked_destination_fails_after_yes_and_keeps_its_bytes() {
    let root = Scratch::new("locked");
    let target = root.0.join("part.dwg");
    let before = saved(&target, 9.0);
    assert!(std::process::Command::new("chflags")
        .arg("uchg")
        .arg(&target)
        .status()
        .unwrap()
        .success());
    let mut s = Session::default();
    point(&mut s, 1.0);
    commands(&mut s, &["WBLOCK", target.to_str().unwrap(), "Y"]);
    let error = s.command("*").unwrap_err();
    assert!(error.contains("locked"), "{error}");
    assert_eq!(std::fs::read(&target).unwrap(), before);
    assert_eq!(root.names(), ["part.dwg"]);
}

/// The open drawing's file gets its own question. Declining changes
/// nothing; `Y` replaces it and the document stays attached, now dirty
/// against what its file holds, so QUIT still asks and END rewrites it.
#[test]
fn the_open_drawing_file_needs_its_own_confirmation() {
    let root = Scratch::new("open");
    let path = root.0.join("HOUSE.DWG");
    let before = saved(&path, 9.0);
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 1.0);
    let drawing = s.drawing().clone();
    commands(&mut s, &["WBLOCK", path.to_str().unwrap()]);
    assert_eq!(s.prompt(), OPEN_QUESTION);
    commands(&mut s, &["N"]);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(s.drawing(), &drawing);
    assert!(s.is_dirty());

    // Export only the new POINT over the open drawing's own file.
    commands(
        &mut s,
        &["WBLOCK", path.to_str().unwrap(), "Y", "", "0,0", "2"],
    );
    assert_eq!(points(&path), 1);
    assert_eq!(s.drawing(), &drawing);
    assert_eq!(s.document_path(), Some(path.as_path()));
    assert_eq!(s.document_format(), Some("AC1.40"));
    assert!(s.is_dirty(), "the file no longer holds the drawing");
    assert_eq!(root.names(), ["HOUSE.DWG"], "WBLOCK makes no backup");
    assert!(s.command("END").unwrap());
    assert_eq!(points(&path), 2);
}

#[test]
fn a_whole_drawing_export_over_the_open_file_leaves_it_clean() {
    let root = Scratch::new("open-whole");
    let path = root.0.join("HOUSE.DWG");
    saved(&path, 9.0);
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 1.0);
    commands(&mut s, &["WBLOCK", path.to_str().unwrap(), "Y", "*"]);
    assert_eq!(points(&path), 2);
    assert!(!s.is_dirty(), "the file now holds the drawing");
}

#[cfg(unix)]
#[test]
fn the_open_drawing_is_recognised_through_a_symlink_and_relative_names() {
    let root = Scratch::new("open-alias");
    let path = root.0.join("HOUSE.DWG");
    saved(&path, 9.0);
    let link = root.0.join("alias.dwg");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    let mut s = Session::open(&path, &[]).unwrap();
    commands(&mut s, &["WBLOCK", link.to_str().unwrap()]);
    assert_eq!(s.prompt(), OPEN_QUESTION);
    s.cancel().unwrap();
    let dotted = root.0.join(".").join("HOUSE");
    commands(&mut s, &["WBLOCK", dotted.to_str().unwrap()]);
    assert_eq!(s.prompt(), OPEN_QUESTION);
    s.cancel().unwrap();
    // Same file under a different case on a case-insensitive volume
    // (the macOS default); a case-sensitive volume has no such alias.
    let other_case = root.0.join("house.dwg");
    if other_case.exists() {
        commands(&mut s, &["WBLOCK", other_case.to_str().unwrap()]);
        assert_eq!(s.prompt(), OPEN_QUESTION);
        s.cancel().unwrap();
    }
}

#[test]
fn api_command_follows_the_same_question() {
    let root = Scratch::new("api");
    let target = root.0.join("part.dwg");
    let before = saved(&target, 9.0);
    let mut s = Session::default();
    point(&mut s, 1.0);
    let send = |s: &mut Session, input: &str| {
        let request =
            serde_json::from_value(json!({"method":"command","params":{"input":input}})).unwrap();
        api::dispatch(s, request, (800, 600)).unwrap()
    };
    send(&mut s, "WBLOCK");
    let state = send(&mut s, target.to_str().unwrap());
    assert_eq!(state["state"]["prompt"], QUESTION, "{state}");
    send(&mut s, "N");
    assert_eq!(std::fs::read(&target).unwrap(), before);
    for input in ["WBLOCK", target.to_str().unwrap(), "Y", "*"] {
        send(&mut s, input);
    }
    assert_eq!(points(&target), 1);
}

/// A script item answers the question; a script written for a new file
/// (`WBLOCK name *`) does not auto-confirm: its `*` declines, as in the
/// original, and the following items run as commands.
#[test]
fn a_script_answers_the_question_with_its_next_item() {
    let root = Scratch::new("script");
    let target = root.0.join("part.dwg");
    let before = saved(&target, 9.0);
    let name = target.to_str().unwrap();
    assert!(!name.contains(char::is_whitespace));
    let mut s = Session::default();
    point(&mut s, 1.0);
    let declining = root.0.join("NO.SCR");
    std::fs::write(&declining, format!("WBLOCK {name} *\nPOINT 5,5\n")).unwrap();
    s.start_script(declining.to_str().unwrap()).unwrap();
    s.pump_script();
    assert!(s.script_phase().is_none(), "{}", s.status());
    assert_eq!(std::fs::read(&target).unwrap(), before);
    assert_eq!(s.drawing().entities().count(), 2);
    let confirming = root.0.join("YES.SCR");
    std::fs::write(&confirming, format!("WBLOCK {name} Y *\n")).unwrap();
    s.start_script(confirming.to_str().unwrap()).unwrap();
    s.pump_script();
    assert_eq!(points(&target), 2);
}
