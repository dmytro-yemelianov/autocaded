use acad_app::{api, Session};
use acad_model::Point;
use serde_json::json;
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("acad-lifecycle-{}-{label}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn commands(s: &mut Session, values: &[&str]) {
    for v in values {
        assert!(!s.command(v).unwrap());
    }
}
fn point(s: &mut Session) {
    commands(s, &["POINT"]);
    s.point(Point { x: 3.0, y: 4.0 }).unwrap();
}
/// Directory entries other than `.BAK` backups (docs/native-files-menu.md).
fn non_backup_entries(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter(|entry| {
            !Path::new(&entry.as_ref().unwrap().file_name())
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("bak"))
        })
        .count()
}
fn open(path: &Path) -> Session {
    Session::open(path, &[]).unwrap()
}

#[test]
fn dirty_state_tracks_model_changes_undo_reports_and_successful_save_as() {
    let root = Scratch::new("dirty");
    let path = root.0.join("first.dwg");
    let mut s = Session::default();
    assert!(!s.is_dirty() && s.document_path().is_none());
    point(&mut s);
    assert!(s.is_dirty() && s.title().contains(" *"));
    commands(&mut s, &["UNDO"]);
    assert!(!s.is_dirty());
    point(&mut s);
    s.save(&path).unwrap();
    assert!(!s.is_dirty());
    assert_eq!(s.document_path(), Some(path.as_path()));
    assert_eq!(s.document_format(), Some("AC1.40"));
    commands(&mut s, &["HELP", "DIM", "STATUS"]);
    assert!(!s.is_dirty());
    commands(&mut s, &["SNAP", "0.5"]);
    assert!(s.is_dirty());
    s.save(&path).unwrap();
    assert!(!s.is_dirty());
    commands(&mut s, &["UNDO"]);
    assert!(s.is_dirty());
    s.reset();
    assert!(!s.is_dirty() && s.document_path().is_none());
}

#[test]
fn end_saves_a_named_drawing_and_an_unnamed_drawing_asks_for_a_path() {
    let root = Scratch::new("end");
    let path = root.0.join("drawing with spaces.dwg");
    let mut s = Session::default();
    point(&mut s);
    assert!(!s.command("END").unwrap());
    assert_eq!(s.prompt(), "END: output file");
    assert!(s.command("").is_err());
    assert_eq!(s.prompt(), "END: output file");
    assert!(s.command(path.to_str().unwrap()).unwrap());
    assert!(!s.is_dirty());
    let mut s = open(&path);
    point(&mut s);
    let expected = s.drawing().items.clone();
    assert!(s.command("END").unwrap());
    assert!(!s.is_dirty());
    assert_eq!(open(&path).drawing().items, expected);
    // END also restores a deleted source file when no model edit occurred.
    std::fs::remove_file(&path).unwrap();
    assert!(s.command("END").unwrap());
    assert!(path.exists());
}

#[test]
fn end_preserves_detected_source_format_and_revision_instead_of_guessing_by_suffix() {
    let root = Scratch::new("formats");
    let original = Session::default();
    for (name, bytes, format) in [
        (
            "legacy.DXF",
            acad_dwg::write_version(original.drawing(), acad_dwg::header::Version::Ac12).unwrap(),
            "AC1.2",
        ),
        (
            "current.BAK",
            acad_dwg::write(original.drawing()).unwrap(),
            "AC1.40",
        ),
        ("exchange.DWG", acad_dxf::write(original.drawing()), "dxf"),
    ] {
        let path = root.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        let mut s = open(&path);
        assert_eq!(s.document_format(), Some(format));
        point(&mut s);
        assert!(s.command("END").unwrap());
        let reopened = open(&path);
        assert_eq!(reopened.document_format(), Some(format));
        assert_eq!(reopened.drawing().items, s.drawing().items);
    }
}

#[test]
fn failed_end_keeps_original_bytes_dirty_state_and_document_path_then_save_as_recovers() {
    let root = Scratch::new("failed");
    let path = root.0.join("original.dwg");
    let mut s = Session::default();
    s.save(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    commands(&mut s, &["TEXT", "0,0", "1", "0", "snowman ☃"]);
    assert!(s.command("END").unwrap_err().contains("Latin-1"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(s.is_dirty());
    assert_eq!(s.document_path(), Some(path.as_path()));
    assert_eq!(s.prompt(), "Command");
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    let failed = root.0.join("missing/new.dwg");
    assert!(s.save(&failed).is_err());
    assert_eq!(s.document_path(), Some(path.as_path()));
    let dxf = root.0.join("recovery.DXF");
    s.save(&dxf).unwrap();
    assert!(!s.is_dirty());
    assert_eq!(s.document_path(), Some(dxf.as_path()));
    assert_eq!(s.document_format(), Some("dxf"));
}

#[test]
fn quit_confirmation_window_close_and_explicit_api_discard_never_write_the_source() {
    let root = Scratch::new("quit");
    let path = root.0.join("source.dwg");
    let mut s = Session::default();
    s.save(&path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    commands(&mut s, &["LINE", "0,0", "1,1"]);
    assert!(!s.request_quit().unwrap());
    assert!(s.prompt().starts_with("QUIT:"));
    assert!(!s.command("N").unwrap());
    assert_eq!(s.prompt(), "Command");
    assert!(s.is_dirty());
    assert!(!s.command("QUIT").unwrap());
    s.cancel().unwrap();
    assert_eq!(s.prompt(), "Command");
    let result = api::dispatch(
        &mut s,
        serde_json::from_value(json!({"method":"quit","params":{}})).unwrap(),
        (640, 480),
    )
    .unwrap();
    assert_eq!(result["quit"], false);
    assert_eq!(result["state"]["dirty"], true);
    assert!(s.command("yes").unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let result = api::dispatch(
        &mut s,
        serde_json::from_value(json!({"method":"quit","params":{"discard":true}})).unwrap(),
        (640, 480),
    )
    .unwrap();
    assert_eq!(result["quit"], true);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn wblock_does_not_rebind_the_current_path_or_clear_dirty_state() {
    let root = Scratch::new("wblock");
    let path = root.0.join("source.dwg");
    let export = root.0.join("export.dwg");
    let mut s = Session::default();
    s.save(&path).unwrap();
    point(&mut s);
    commands(&mut s, &["WBLOCK", export.to_str().unwrap(), "*"]);
    assert!(export.exists() && s.is_dirty());
    assert_eq!(s.document_path(), Some(path.as_path()));
}

#[cfg(unix)]
#[test]
fn staged_save_preserves_permissions_and_follows_existing_and_dangling_symlinks() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = Scratch::new("symlink");
    let target = root.0.join("target.dwg");
    let link = root.0.join("alias.dwg");
    let mut s = Session::default();
    s.save(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
    symlink("target.dwg", &link).unwrap();
    point(&mut s);
    s.save(&link).unwrap();
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(open(&target).drawing().items, s.drawing().items);
    let dangling = root.0.join("dangling.dwg");
    symlink("new.dwg", &dangling).unwrap();
    s.save(&dangling).unwrap();
    assert!(std::fs::symlink_metadata(&dangling).unwrap().is_symlink());
    assert!(root.0.join("new.dwg").is_file());
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o440)).unwrap();
    let bytes = std::fs::read(&target).unwrap();
    point(&mut s);
    assert!(s.save(&link).is_err());
    assert!(s.is_dirty());
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    // target.bak is the backup kept when the link's target was replaced.
    assert!(root.0.join("target.bak").is_file());
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 5);
}

#[test]
fn supported_signed_off_persists_through_end_save_wblock_and_api() {
    let root = Scratch::new("signed-off");
    let mut base = Session::default();
    commands(
        &mut base,
        &[
            "POINT",
            "3,4",
            "LAYER",
            "2",
            "LAYER COLOR 7",
            "POINT",
            "6,8",
        ],
    );
    let cases = [
        (
            "old.dwg",
            acad_dwg::write_version(base.drawing(), acad_dwg::header::Version::Ac12).unwrap(),
            "AC1.2",
        ),
        (
            "current.dwg",
            acad_dwg::write(base.drawing()).unwrap(),
            "AC1.40",
        ),
        (
            "exchange.dxf",
            acad_dxf::try_write(base.drawing()).unwrap(),
            "dxf",
        ),
    ];
    for (index, (name, bytes, format)) in cases.into_iter().enumerate() {
        let path = root.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        let mut s = open(&path);
        let original = s.drawing().clone();
        commands(&mut s, &["LAYER OFF 2", "LAYER", "2"]);
        let hidden = s.drawing().clone();
        assert!(s.is_dirty());
        assert_eq!(hidden.header.current_layer, 2);
        assert!(!hidden.header.layer_is_visible(2));
        let exported = root.0.join(format!("selected-{index}.dwg"));
        commands(
            &mut s,
            &["WBLOCK", exported.to_str().unwrap(), "", "0,0", "ALL"],
        );
        let block = open(&exported);
        assert_eq!(block.drawing().header.off_layers, hidden.header.off_layers);
        assert_eq!(block.drawing().header.layers, hidden.header.layers);
        assert_eq!(block.drawing().items, hidden.items);
        assert_eq!(s.document_path(), Some(path.as_path()));
        assert!(s.is_dirty());
        let exchange = api::dispatch(&mut s, api::Request::Drawing {}, (800, 600)).unwrap();
        let decoded = acad_dxf::parse(exchange["data"].as_str().unwrap().as_bytes()).unwrap();
        assert_eq!(decoded.header.off_layers, hidden.header.off_layers);
        assert_eq!(decoded.header.layers, hidden.header.layers);
        assert!(s.command("END").unwrap());
        assert!(!s.is_dirty());
        assert_eq!(s.document_format(), Some(format));
        let mut reopened = open(&path);
        assert_eq!(reopened.document_format(), Some(format));
        assert_eq!(
            reopened.drawing().header.off_layers,
            hidden.header.off_layers
        );
        assert_eq!(reopened.drawing().header.layers, hidden.header.layers);
        assert_eq!(reopened.drawing().header.current_layer, 2);
        assert_eq!(reopened.drawing().items, hidden.items);
        assert_eq!(api::state(&reopened)["off_layers"], json!([2]));
        assert_eq!(
            &s.frame(800, 600).unwrap().pixels[..800 * 556],
            &reopened.frame(800, 600).unwrap().pixels[..800 * 556]
        );
        commands(&mut reopened, &["ERASE"]);
        let view = reopened.drawing().header.view;
        let viewport = acad_render::Viewport::from_view(view.center, view.height, 800, 556);
        for (world, expected) in [
            (Point { x: 6.0, y: 8.0 }, ""),
            (Point { x: 3.0, y: 4.0 }, "1"),
        ] {
            let screen = viewport.to_screen(world);
            api::dispatch(
                &mut reopened,
                api::Request::Click {
                    x: screen.x,
                    y: screen.y,
                    width: None,
                    height: None,
                },
                (800, 600),
            )
            .unwrap();
            assert_eq!(
                reopened.input(),
                expected,
                "reopened picks must share persisted visibility"
            );
        }
        reopened.cancel().unwrap();
        assert!(!reopened.is_dirty());
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &original);
        assert!(
            s.is_dirty(),
            "undo changes the successfully saved OFF baseline"
        );
        commands(&mut s, &["LAYER OFF 2", "ID"]);
        let prompt = s.prompt().to_owned();
        let save_as = root.0.join(format!("api-save-{index}.dwg"));
        api::dispatch(
            &mut s,
            api::Request::Save {
                path: save_as.to_str().unwrap().into(),
            },
            (800, 600),
        )
        .unwrap();
        assert_eq!(s.prompt(), prompt);
        assert!(!s.is_dirty());
        assert_eq!(s.document_path(), Some(save_as.as_path()));
        assert_eq!(
            open(&save_as).drawing().header.off_layers,
            hidden.header.off_layers
        );
        s.cancel().unwrap();
        commands(&mut s, &["SAVE", path.to_str().unwrap()]);
        assert!(!s.is_dirty());
        assert_eq!(s.document_path(), Some(path.as_path()));
        assert_eq!(
            open(&path).drawing().header.off_layers,
            hidden.header.off_layers
        );
    }
}

#[test]
fn zero_color_off_failures_preserve_destination_attachment_dirty_prompt_and_undo() {
    let root = Scratch::new("layer-encoding");
    let mut base = Session::default();
    commands(&mut base, &["LAYER COLOR 0"]);
    point(&mut base);
    let cases = [
        (
            "old.dwg",
            acad_dwg::write_version(base.drawing(), acad_dwg::header::Version::Ac12).unwrap(),
            "AC1.2",
        ),
        (
            "current.dwg",
            acad_dwg::write(base.drawing()).unwrap(),
            "AC1.40",
        ),
        (
            "exchange.dxf",
            acad_dxf::try_write(base.drawing()).unwrap(),
            "dxf",
        ),
    ];
    for (index, (name, saved_bytes, format)) in cases.into_iter().enumerate() {
        let path = root.0.join(name);
        let destination = root.0.join(format!("other-{name}"));
        let exported = root.0.join(format!("block-{index}.dwg"));
        for target in [&path, &destination, &exported] {
            std::fs::write(target, &saved_bytes).unwrap();
        }
        let mut s = open(&path);
        let saved_drawing = s.drawing().clone();
        commands(&mut s, &["LAYER OFF 1"]);
        let hidden = s.drawing().clone();
        assert!(s.is_dirty());
        commands(&mut s, &["SAVE"]);
        assert!(s
            .command(destination.to_str().unwrap())
            .unwrap_err()
            .contains("color 1..127"));
        assert_eq!(s.prompt(), "Command");
        commands(&mut s, &["ID"]);
        let prompt = s.prompt().to_owned();
        s.set_input("pending text".into());
        for request in [
            api::Request::Save {
                path: destination.to_str().unwrap().into(),
            },
            api::Request::Drawing {},
        ] {
            assert!(api::dispatch(&mut s, request, (800, 600))
                .unwrap_err()
                .contains("color 1..127"));
            assert_eq!(s.prompt(), prompt);
            assert_eq!(s.input(), "pending text");
            assert_eq!(s.drawing(), &hidden);
        }
        s.cancel().unwrap();
        assert!(s.command("END").unwrap_err().contains("color 1..127"));
        commands(&mut s, &["WBLOCK", exported.to_str().unwrap()]);
        assert!(s.command("*").unwrap_err().contains("color 1..127"));
        assert_eq!(s.drawing(), &hidden);
        assert!(s.is_dirty());
        assert_eq!(s.document_path(), Some(path.as_path()));
        assert_eq!(s.document_format(), Some(format));
        for target in [&path, &destination, &exported] {
            assert_eq!(std::fs::read(target).unwrap(), saved_bytes);
        }
        assert_eq!(
            non_backup_entries(&root.0),
            3 * (index + 1),
            "no staging files"
        );
        s.cancel().unwrap();
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &saved_drawing);
        assert!(!s.is_dirty());
        commands(&mut s, &["LAYER OFF 1", "LAYER ON 1"]);
        s.save(&path).unwrap();
        let reopened = open(&path);
        assert!(reopened.drawing().header.off_layers.is_empty());
        assert_eq!(reopened.drawing().header.layers[&1], 0);
    }
}

#[test]
fn nested_repeat_command_and_api_save_end_round_trip_and_malformed_save_is_atomic() {
    let root = Scratch::new("nested-repeat-encoding");
    let mut flat = Session::default();
    commands(
        &mut flat,
        &["REPEAT", "POINT", "1,1", "ENDREP", "2", "1", "3", "0"],
    );
    let cases = [
        (
            "legacy.dwg",
            acad_dwg::write_version(flat.drawing(), acad_dwg::header::Version::Ac12).unwrap(),
            "AC1.2",
        ),
        (
            "current.dwg",
            acad_dwg::write(flat.drawing()).unwrap(),
            "AC1.40",
        ),
        (
            "exchange.dxf",
            acad_dxf::try_write(flat.drawing()).unwrap(),
            "dxf",
        ),
    ];
    for (case_index, (name, original_bytes, format)) in cases.into_iter().enumerate() {
        let source = root.0.join(name);
        let destination = root.0.join(format!("other-{name}"));
        std::fs::write(&source, &original_bytes).unwrap();
        std::fs::write(&destination, &original_bytes).unwrap();
        let mut s = open(&source);
        commands(
            &mut s,
            &[
                "REPEAT", "ARRAY", "1", "R", "1", "2", "0", "10", "ENDREP", "1", "1", "0", "0",
            ],
        );
        let nested = s.drawing().clone();
        assert!(s.command("END").unwrap());
        assert_eq!(s.document_format(), Some(format));
        assert_eq!(open(&source).drawing().items, nested.items);
        assert!(!s.is_dirty());
        let saved_bytes = std::fs::read(&source).unwrap();
        // Native live nesting is writable; malformed derived spacings remain a checked boundary.
        commands(&mut s, &["SCALE", "2", "0,0", "1e308"]);
        let malformed = s.drawing().clone();
        commands(&mut s, &["SAVE"]);
        assert_eq!(s.prompt(), "SAVE: output file");
        assert!(s
            .command(destination.to_str().unwrap())
            .unwrap_err()
            .contains("finite spacings"));
        assert_eq!(s.prompt(), "Command");
        commands(&mut s, &["ID"]);
        let prompt = s.prompt().to_owned();
        for request in [
            json!({"method":"save","params":{"path":destination}}),
            json!({"method":"drawing","params":{}}),
        ] {
            assert!(
                api::dispatch(&mut s, serde_json::from_value(request).unwrap(), (800, 600))
                    .unwrap_err()
                    .contains("finite spacings")
            );
            assert_eq!(s.prompt(), prompt);
            assert_eq!(s.drawing(), &malformed);
        }
        s.cancel().unwrap();
        assert!(s.command("END").unwrap_err().contains("finite spacings"));
        assert!(api::dispatch(
            &mut s,
            serde_json::from_value(json!({"method":"command","params":{"input":"END"}})).unwrap(),
            (800, 600)
        )
        .unwrap_err()
        .contains("finite spacings"));
        assert_eq!(s.drawing(), &malformed);
        assert!(s.is_dirty());
        assert_eq!(s.document_path(), Some(source.as_path()));
        assert_eq!(s.document_format(), Some(format));
        assert_eq!(std::fs::read(&source).unwrap(), saved_bytes);
        assert_eq!(std::fs::read(&destination).unwrap(), original_bytes);
        assert_eq!(non_backup_entries(&root.0), 2 * (case_index + 1));
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &nested, "failed writes added no undo entry");
        assert!(!s.is_dirty());
        assert!(s.command("END").unwrap());
        assert_eq!(open(&source).drawing().items, nested.items);
        // Save As is now successful on the same nested model after recovery.
        s.save(&destination).unwrap();
        assert_eq!(open(&destination).drawing().items, nested.items);
    }
}
