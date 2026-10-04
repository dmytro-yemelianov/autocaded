//! Native `.BAK` policy (docs/native-files-menu.md). The original's END
//! backup behaviour is measured in crates/acad-oracle/tests/files_backup.rs;
//! host naming, symlink, permission and failure handling are Rust policy.
use acad_app::Session;
use acad_model::Point;
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("acad-backups-{}-{label}", std::process::id()));
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
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn point(s: &mut Session, x: f64) {
    assert!(!s.command("POINT").unwrap());
    s.point(Point { x, y: 4.0 }).unwrap();
}

/// A saved drawing at `path` holding one POINT, and its bytes.
fn saved(path: &Path) -> Vec<u8> {
    let mut s = Session::default();
    point(&mut s, 1.0);
    s.save(path).unwrap();
    std::fs::read(path).unwrap()
}

#[test]
fn end_keeps_previous_bytes_as_bak_and_replaces_an_existing_bak() {
    let root = Scratch::new("end");
    let path = root.0.join("HOUSE.DWG");
    let before = saved(&path);
    std::fs::write(root.0.join("HOUSE.BAK"), b"stale backup").unwrap();
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 2.0);
    assert!(s.command("END").unwrap());
    assert_eq!(std::fs::read(root.0.join("HOUSE.BAK")).unwrap(), before);
    let reopened = Session::open(&path, &[]).unwrap();
    assert_eq!(reopened.drawing().items, s.drawing().items);
    assert_eq!(root.names(), ["HOUSE.BAK", "HOUSE.DWG"]);
}

#[test]
fn unchanged_end_still_writes_a_backup_and_a_second_end_backs_up_the_first() {
    let root = Scratch::new("twice");
    let path = root.0.join("D2.DWG");
    let before = saved(&path);
    let mut s = Session::open(&path, &[]).unwrap();
    assert!(s.command("END").unwrap());
    assert_eq!(std::fs::read(root.0.join("D2.BAK")).unwrap(), before);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 2.0);
    assert!(s.command("END").unwrap());
    let first = std::fs::read(&path).unwrap();
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 3.0);
    assert!(s.command("END").unwrap());
    assert_eq!(std::fs::read(root.0.join("D2.BAK")).unwrap(), first);
}

#[test]
fn a_new_destination_gets_no_backup_and_quit_and_failed_encoding_write_none() {
    let root = Scratch::new("new");
    let path = root.0.join("fresh.dwg");
    let mut s = Session::default();
    point(&mut s, 1.0);
    assert!(!s.command("END").unwrap());
    assert!(s.command(path.to_str().unwrap()).unwrap());
    assert_eq!(root.names(), ["fresh.dwg"]);

    let before = std::fs::read(&path).unwrap();
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 2.0);
    assert!(!s.request_quit().unwrap());
    assert!(s.command("Y").unwrap());
    assert_eq!(root.names(), ["fresh.dwg"]);

    let mut s = Session::open(&path, &[]).unwrap();
    for input in ["TEXT", "0,0", "1", "0", "snowman ☃"] {
        s.command(input).unwrap();
    }
    assert!(s.command("END").unwrap_err().contains("Latin-1"));
    assert_eq!(root.names(), ["fresh.dwg"]);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn backup_names_follow_the_destination_extension_case() {
    let root = Scratch::new("names");
    for (name, backup) in [
        ("plan.dxf", "plan.bak"),
        ("PLAN", "PLAN.BAK"),
        ("Mixed.Dwg", "Mixed.BAK"),
    ] {
        let path = root.0.join(name);
        let before = saved(&path);
        let mut s = Session::open(&path, &[]).unwrap();
        point(&mut s, 2.0);
        s.save(&path).unwrap();
        assert_eq!(
            std::fs::read(root.0.join(backup)).unwrap(),
            before,
            "{name}"
        );
    }
    // Save As over another existing file backs that file up as well.
    let other = root.0.join("other.dwg");
    let before = saved(&other);
    let mut s = Session::open(&root.0.join("PLAN"), &[]).unwrap();
    s.save(&other).unwrap();
    assert_eq!(std::fs::read(root.0.join("other.bak")).unwrap(), before);
}

#[test]
fn a_bak_destination_is_replaced_without_a_backup_of_itself() {
    let root = Scratch::new("bak");
    let path = root.0.join("DISC.BAK");
    saved(&path);
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 2.0);
    assert!(s.command("END").unwrap());
    assert_eq!(root.names(), ["DISC.BAK"]);
    assert_eq!(
        Session::open(&path, &[]).unwrap().drawing().items,
        s.drawing().items
    );
}

#[test]
fn backups_keep_the_source_revision_bytes_while_end_keeps_the_codec() {
    let root = Scratch::new("revision");
    let path = root.0.join("legacy.dwg");
    let source = acad_dwg::write_version(
        Session::default().drawing(),
        acad_dwg::header::Version::Ac12,
    )
    .unwrap();
    std::fs::write(&path, &source).unwrap();
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 2.0);
    assert!(s.command("END").unwrap());
    assert_eq!(std::fs::read(root.0.join("legacy.bak")).unwrap(), source);
    assert_eq!(
        Session::open(&path, &[]).unwrap().document_format(),
        Some("AC1.2")
    );
}

#[test]
fn a_backup_that_cannot_be_written_fails_the_save_and_keeps_everything() {
    let root = Scratch::new("blocked");
    let path = root.0.join("D.DWG");
    let before = saved(&path);
    std::fs::create_dir(root.0.join("D.BAK")).unwrap();
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 2.0);
    assert!(s.command("END").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(root.0.join("D.BAK").is_dir());
    assert!(s.is_dirty());
    assert_eq!(s.document_path(), Some(path.as_path()));
    assert_eq!(s.prompt(), "Command");
    assert_eq!(root.names(), ["D.BAK", "D.DWG"]);
}

#[cfg(unix)]
#[test]
fn read_only_directory_permissions_and_symlinks_are_preserved() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = Scratch::new("unix");
    let store = root.0.join("store");
    std::fs::create_dir(&store).unwrap();
    let real = store.join("real.dwg");
    let before = saved(&real);
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).unwrap();

    // Read-only directory: nothing is written, nothing is lost.
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o555)).unwrap();
    let mut s = Session::open(&real, &[]).unwrap();
    point(&mut s, 2.0);
    let failed = s.command("END");
    std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(failed.is_err());
    assert_eq!(std::fs::read(&real).unwrap(), before);
    assert!(s.is_dirty());
    let names: Vec<_> = std::fs::read_dir(&store).unwrap().collect();
    assert_eq!(names.len(), 1);

    // A symlinked destination: the link stays, the backup sits beside the
    // resolved file and both files keep the destination permissions.
    let link = root.0.join("alias.dwg");
    symlink(&real, &link).unwrap();
    let mut s = Session::open(&link, &[]).unwrap();
    point(&mut s, 3.0);
    assert!(s.command("END").unwrap());
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert!(!root.0.join("alias.bak").exists());
    let backup = store.join("real.bak");
    assert_eq!(std::fs::read(&backup).unwrap(), before);
    for path in [&real, &backup] {
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    // A symlink at the backup name is replaced as a link, never followed.
    let victim = root.0.join("victim.txt");
    std::fs::write(&victim, b"keep me").unwrap();
    std::fs::remove_file(&backup).unwrap();
    symlink(&victim, &backup).unwrap();
    let previous = std::fs::read(&real).unwrap();
    let mut s = Session::open(&real, &[]).unwrap();
    point(&mut s, 4.0);
    assert!(s.command("END").unwrap());
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");
    assert!(!std::fs::symlink_metadata(&backup).unwrap().is_symlink());
    assert_eq!(std::fs::read(&backup).unwrap(), previous);
}

#[test]
fn wblock_over_an_existing_file_makes_no_backup() {
    let root = Scratch::new("wblock");
    let target = root.0.join("part.dwg");
    saved(&target);
    let mut s = Session::default();
    point(&mut s, 2.0);
    for input in ["WBLOCK", target.to_str().unwrap(), "*"] {
        s.command(input).unwrap();
    }
    assert_eq!(root.names(), ["part.dwg"]);
}

/// Finder "Locked" (`chflags uchg`): refused before anything is written,
/// keeping the destination and the old backup, with no staging residue.
#[cfg(target_os = "macos")]
#[test]
fn a_locked_destination_fails_without_residue() {
    struct Unlock(PathBuf);
    impl Drop for Unlock {
        fn drop(&mut self) {
            let _ = std::process::Command::new("chflags")
                .arg("-R")
                .arg("nouchg")
                .arg(&self.0)
                .status();
        }
    }
    let root = Scratch::new("locked");
    let path = root.0.join("D.DWG");
    let before = saved(&path);
    std::fs::write(root.0.join("D.BAK"), b"older backup").unwrap();
    let _unlock = Unlock(root.0.clone());
    assert!(std::process::Command::new("chflags")
        .arg("uchg")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    let mut s = Session::open(&path, &[]).unwrap();
    point(&mut s, 2.0);
    let error = s.command("END").unwrap_err();
    assert!(error.contains("output file is locked"), "{error}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        std::fs::read(root.0.join("D.BAK")).unwrap(),
        b"older backup"
    );
    assert!(s.is_dirty());
    assert_eq!(root.names(), ["D.BAK", "D.DWG"]);
}
