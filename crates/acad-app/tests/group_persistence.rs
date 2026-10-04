use acad_app::Session;
use acad_model::{Entity, Item};
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let p = std::env::temp_dir().join(format!("acad-groups-{}-{label}", std::process::id()));
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
fn group(s: &mut Session) {
    commands(
        s,
        &[
            "LAYER 2", "REPEAT", "LAYER 3", "POINT", "1,2", "LAYER 4", "ENDREP", "2", "1", "3", "0",
        ],
    );
}
#[test]
fn command_marker_layers_are_metadata_and_group_edits_round_trip() {
    let root = Scratch::new("edits");
    let mut s = Session::default();
    group(&mut s);
    let Item::Repeat(r) = &s.drawing().items[0] else {
        panic!("pattern")
    };
    assert_eq!((r.start_layer, r.end_layer), (2, 4));
    assert!(matches!(&r.entities[0], Entity::OnLayer { layer: 3, .. }));
    // No runtime owner gate is introduced by opening-layer metadata.
    let vp = acad_render::Viewport::fit(&s.drawing().header.limits, 800, 600);
    let visible = acad_render::flatten(s.drawing(), &vp);
    commands(&mut s, &["LAYER OFF 2"]);
    assert_eq!(acad_render::flatten(s.drawing(), &vp), visible);
    commands(&mut s, &["LAYER ON 2"]);
    commands(
        &mut s,
        &[
            "MOVE", "1,0", "", "1", "COPY", "10,0", "", "1", "SCALE", "2", "0,0", "2",
        ],
    );
    let expected = s.drawing().items.clone();
    for (name, bytes) in [
        (
            "legacy.dwg",
            acad_dwg::write_version(s.drawing(), acad_dwg::header::Version::Ac12).unwrap(),
        ),
        ("current.dwg", acad_dwg::write(s.drawing()).unwrap()),
        ("exchange.dxf", acad_dxf::try_write(s.drawing()).unwrap()),
    ] {
        let path = root.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(Session::open(&path, &[]).unwrap().drawing().items, expected);
    }
}
#[test]
fn nested_array_group_and_named_block_wblock_reopen_with_dependencies() {
    let root = Scratch::new("block");
    let mut s = Session::default();
    group(&mut s);
    commands(
        &mut s,
        &[
            "POINT", "0,0", "BLOCK", "LEAF", "0,0", "2", "REPEAT", "INSERT", "LEAF", "0,0", "1",
            "1", "0", "ENDREP", "2", "1", "3", "0",
        ],
    );
    commands(
        &mut s,
        &[
            "REPEAT", "ARRAY", "2", "R", "1", "2", "0", "10", "ENDREP", "1", "1", "0", "0",
        ],
    );
    let live = s.drawing().clone();
    let path = root.0.join("nested.dwg");
    // The ordinary erased block source remains independently encodable as DWG.
    s.save(&path).unwrap();
    assert_eq!(
        Session::open(&path, &[]).unwrap().drawing().items,
        live.items
    );
    commands(&mut s, &["BLOCK", "GROUP", "0,0", "3"]);
    let blocked = s.drawing().clone();
    let blocked_path = root.0.join("blocked.dwg");
    s.save(&blocked_path).unwrap();
    assert!(s.command("END").unwrap());
    assert_eq!(
        Session::open(&blocked_path, &[]).unwrap().drawing().items,
        blocked.items
    );
    for version in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ] {
        let bytes = acad_dwg::write_version(&blocked, version).unwrap();
        assert_eq!(acad_dwg::parse(&bytes).unwrap().items, blocked.items);
    }
    let live_items: Vec<_> = blocked
        .items
        .iter()
        .filter(|item| !matches!(item, Item::Erased(_)))
        .cloned()
        .collect();
    assert_eq!(
        acad_dxf::parse(&acad_dxf::try_write(&blocked).unwrap())
            .unwrap()
            .items,
        live_items
    );
    let block = s.drawing().block("GROUP").unwrap().clone();
    let export = root.0.join("group.dwg");
    commands(&mut s, &["WBLOCK", export.to_str().unwrap(), "GROUP"]);
    let reopened = Session::open(&export, &[]).unwrap();
    let Entity::Repeat(exported_group) = &block.entities[0] else {
        panic!("group body")
    };
    assert_eq!(
        reopened.drawing().items.last(),
        Some(&Item::Repeat(exported_group.clone()))
    );
    assert!(reopened.drawing().block("LEAF").is_some());
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &live);
}
#[test]
fn erased_groups_save_end_and_session_oops_undo_retain_same_content() {
    let root = Scratch::new("erased");
    for (name, version) in [
        ("old.dwg", acad_dwg::header::Version::Ac12),
        ("new.dwg", acad_dwg::header::Version::Ac140),
    ] {
        let mut s = Session::default();
        group(&mut s);
        commands(
            &mut s,
            &[
                "REPEAT", "ARRAY", "1", "R", "1", "2", "0", "10", "ENDREP", "1", "1", "0", "0",
            ],
        );
        let path = root.0.join(name);
        std::fs::write(
            &path,
            acad_dwg::write_version(s.drawing(), version).unwrap(),
        )
        .unwrap();
        let mut s = Session::open(&path, &[]).unwrap();
        let live = s.drawing().clone();
        commands(&mut s, &["ERASE", "ALL"]);
        let erased = s.drawing().clone();
        assert!(s.command("END").unwrap());
        assert!(!s.is_dirty());
        assert_eq!(
            Session::open(&path, &[]).unwrap().drawing().items,
            erased.items
        );
        assert_eq!(
            acad_dwg::header::parse_header(&std::fs::read(&path).unwrap())
                .unwrap()
                .1
                .version,
            version
        );
        assert!(s.command("END").unwrap());
        commands(&mut s, &["OOPS"]);
        assert_eq!(s.drawing(), &live);
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &erased);
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &live);
        assert!(s.command("END").unwrap());
        assert_eq!(
            Session::open(&path, &[]).unwrap().drawing().items,
            live.items
        );
        commands(&mut s, &["ERASE", "ALL"]);
        let before_export = s.drawing().clone();
        let dxf = root.0.join(format!("{name}.dxf"));
        s.save(&dxf).unwrap();
        assert_eq!(
            s.drawing(),
            &before_export,
            "live-only exchange does not mutate session history"
        );
        assert!(Session::open(&dxf, &[]).unwrap().drawing().items.is_empty());
        assert!(s.command("END").unwrap());
        commands(&mut s, &["OOPS"]);
        assert_eq!(s.drawing(), &live);
    }
}

#[test]
fn checked_api_live_export_and_native_save_preserve_pending_input_and_session_history() {
    use acad_app::api;
    use serde_json::json;
    let root = Scratch::new("api");
    let mut s = Session::default();
    group(&mut s);
    let live = s.drawing().clone();
    commands(&mut s, &["ERASE", "1", "POINT"]);
    let erased = s.drawing().clone();
    let request = |method: &str, params| {
        serde_json::from_value(json!({"method": method, "params": params})).unwrap()
    };
    let exported = api::dispatch(&mut s, request("drawing", json!({})), (800, 600)).unwrap();
    assert!(!exported["data"].as_str().unwrap().contains("REPEAT"));
    let path = root.0.join("erased.dwg");
    api::dispatch(&mut s, request("save", json!({"path": path})), (800, 600)).unwrap();
    assert_eq!(s.drawing(), &erased);
    assert!(s.prompt().contains("POINT"));
    s.cancel().unwrap();
    commands(&mut s, &["OOPS"]);
    assert_eq!(s.drawing(), &live);
    let mut reopened = Session::open(&path, &[]).unwrap();
    let before = reopened.drawing().clone();
    commands(&mut reopened, &["OOPS", "UNDO"]);
    assert_eq!(
        reopened.drawing(),
        &before,
        "opening retains geometry but has no persisted editor restoration stack"
    );
    assert!(matches!(before.items[0], Item::Erased(Entity::Repeat(_))));
}
