//! E2: a layer applied to a whole REPEAT group through the application session
//! (docs/native-group-persistence.md, "E2"). CHANGE layer rewrites the member
//! records and keeps the marker layers, as AutoCAD 1.4 does (oracle
//! `repeat_layer.rs`); END/SAVE/reopen in both DWG revisions and DXF, the
//! rendered frame and window selection agree; one UNDO step reverts it.
use acad_app::Session;
use acad_dwg::header::Version;
use acad_model::{Entity, Item, Point, Repeat};

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let p = std::env::temp_dir().join(format!("acad-e2-{}-{label}", std::process::id()));
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
        assert!(!s.command(input).unwrap(), "{input}");
    }
}
fn on(layer: u8, entity: Entity) -> Entity {
    Entity::OnLayer {
        layer,
        entity: Box::new(entity),
    }
}
fn line(y: f64) -> Entity {
    Entity::Line {
        start: Point { x: 1.0, y },
        end: Point { x: 2.0, y },
    }
}
fn group(member_layer: u8) -> Item {
    Item::Repeat(Repeat {
        start_layer: 2,
        end_layer: 2,
        entities: vec![on(member_layer, line(1.0)), on(member_layer, line(3.0))],
        columns: 2,
        rows: 1,
        column_spacing: 5.0,
        row_spacing: 0.0,
    })
}
/// The oracle fixture: markers drawn on layer 2, members on layer 1.
const FIXTURE: [&str; 17] = [
    "LAYER 2", "REPEAT", "LAYER 1", "LINE", "1,1", "2,1", "", "LINE", "1,3", "2,3", "", "LAYER 2",
    "ENDREP", "2", "1", "5", "0",
];
const CHANGE: [&str; 7] = ["CHANGE", "W", "0,0", "10,5", "", "L", "3"];

fn frame(s: &Session) -> Vec<acad_render::Prim> {
    let vp = acad_render::Viewport::fit(&s.drawing().header.limits, 800, 600);
    acad_render::flatten(s.drawing(), &vp)
}

#[test]
fn change_layer_on_a_group_saves_reopens_and_renders_alike_in_every_format() {
    let root = Scratch::new("formats");
    let mut fresh = Session::default();
    commands(&mut fresh, &FIXTURE);
    assert_eq!(fresh.drawing().items, [group(1)]);
    for (name, version) in [
        ("old.dwg", Some(Version::Ac12)),
        ("new.dwg", Some(Version::Ac140)),
        ("exchange.dxf", None),
    ] {
        let path = root.0.join(name);
        std::fs::write(
            &path,
            match version {
                Some(version) => acad_dwg::write_version(fresh.drawing(), version).unwrap(),
                None => acad_dxf::try_write(fresh.drawing()).unwrap(),
            },
        )
        .unwrap();
        let mut s = Session::open(&path, &[]).unwrap();
        let before = s.drawing().clone();
        commands(&mut s, &CHANGE);
        assert_eq!(s.drawing().items, [group(3)], "{name}");
        let shown = frame(&s);
        assert!(!shown.is_empty());
        // END keeps the attached codec and revision.
        assert!(s.command("END").unwrap());
        assert!(!s.is_dirty());
        if let Some(version) = version {
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(
                acad_dwg::header::parse_header(&bytes).unwrap().1.version,
                version
            );
        }
        let mut reopened = Session::open(&path, &[]).unwrap();
        assert_eq!(reopened.drawing().items, [group(3)], "{name}");
        assert_eq!(frame(&reopened), shown, "{name}");
        // The marker layer gates nothing; the members' new layer does.
        commands(&mut reopened, &["LAYER OFF 2"]);
        assert_eq!(frame(&reopened), shown, "{name}");
        commands(&mut reopened, &["ERASE", "W", "0,0", "10,5", ""]);
        assert!(matches!(reopened.drawing().items[0], Item::Erased(_)));
        commands(&mut reopened, &["OOPS", "LAYER ON 2", "LAYER OFF 3"]);
        assert!(frame(&reopened).is_empty(), "{name}");
        // One UNDO step reverts the CHANGE in the original session.
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &before);
    }
}
