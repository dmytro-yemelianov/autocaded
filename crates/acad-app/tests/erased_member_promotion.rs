//! Native policy for block members promoted to the drawing root by
//! `INSERT *name` and named WBLOCK: an erased member becomes a root erased
//! record with its exact layer/fields, never a live item or a codec failure.
use acad_app::{api, Session};
use acad_dwg::header::Version;
use acad_model::{Drawing, Entity, Item, Point};
use acad_render::{flatten_with_libraries, Libraries, Viewport};

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "acad-erased-promotion-{}-{label}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
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
fn erase(entity: &mut Entity) {
    *entity = Entity::Erased(Box::new(entity.clone()));
}
fn libraries() -> Libraries {
    let mut libraries = Libraries::default();
    libraries
        .insert("TXT", b"*0,4,Vertical\n2,1,0,0\n*65,5,A\n024,2,030,1,0")
        .unwrap();
    libraries
        .insert("ALT", b"*0,4,Horizontal\n4,1,0,0\n*65,5,A\n040,2,010,1,0")
        .unwrap();
    libraries
}
fn render(d: &Drawing) -> Vec<acad_render::Prim> {
    let vp = Viewport::fit(&d.header.limits, 800, 600);
    let frame = flatten_with_libraries(d, &vp, &libraries());
    assert!(frame.diagnostics.is_empty(), "{:?}", frame.diagnostics);
    frame.primitives
}

/// Block B: an erased direct POINT on layer 5, a live POINT and a direct REPEAT
/// member holding one erased and one live POINT. `control` omits both erased leaves.
fn explode_source(control: bool) -> Drawing {
    let mut d = acad_dxf::parse(b"LIMITS,1\r\n0,30,0,30\r\nBLOCK,1\r\n0,0\r\nB\r\nPOINT,5\r\n0,0\r\nPOINT,1\r\n3,3\r\nREPEAT,1\r\nPOINT,6\r\n7,7\r\nPOINT,1\r\n8,8\r\nENDREP,1\r\n1,1,0,0\r\nENDBLK,1\r\nPOINT,1\r\n20,20\r\n").unwrap();
    let Item::Block(b) = &mut d.items[0] else {
        panic!("block")
    };
    if control {
        b.entities.remove(0);
        let Entity::Repeat(r) = &mut b.entities[1] else {
            panic!("member group")
        };
        r.entities.remove(0);
    } else {
        erase(&mut b.entities[0]);
        let Entity::Repeat(r) = &mut b.entities[2] else {
            panic!("member group")
        };
        erase(&mut r.entities[0]);
    }
    d
}

#[test]
fn explode_promotes_erased_members_to_root_erased_records_that_save_and_reopen() {
    let root = Scratch::new("explode");
    for version in [Version::Ac12, Version::Ac140] {
        let tag = format!("{version:?}");
        let open = |control: bool| {
            let path = root.0.join(format!("{tag}-{control}.dwg"));
            std::fs::write(
                &path,
                acad_dwg::write_version(&explode_source(control), version).unwrap(),
            )
            .unwrap();
            let mut s = Session::open(&path, &[]).unwrap();
            let before = s.drawing().clone();
            commands(&mut s, &["INSERT", "*B", "10,10"]);
            (s, before, path)
        };
        let (mut control, _, _) = open(true);
        let (mut s, before, path) = open(false);
        let exploded = s.drawing().clone();
        let added = &exploded.items[before.items.len()..];
        assert_eq!(added.len(), 3);
        assert!(
            matches!(&added[0], Item::Erased(Entity::OnLayer { layer: 5, entity }) if matches!(entity.as_ref(), Entity::Point { origin } if *origin == Point { x: 10.0, y: 10.0 })),
            "erased member is a transformed root erased record: {:?}",
            added[0]
        );
        assert!(matches!(
            &added[1],
            Item::Entity(Entity::OnLayer { layer: 1, .. })
        ));
        let Item::Entity(Entity::Repeat(group)) = &added[2] else {
            panic!("member group stays a group: {:?}", added[2])
        };
        assert!(
            matches!(&group.entities[0], Entity::Erased(e) if matches!(e.as_ref(), Entity::OnLayer { layer: 6, entity } if matches!(entity.as_ref(), Entity::Point { origin } if *origin == Point { x: 17.0, y: 17.0 }))),
            "nested erased member keeps its group and status"
        );
        // Live behavior equals the explode of a block without the erased leaves.
        assert_eq!(exploded.header.extents, control.drawing().header.extents);
        assert_eq!(render(&exploded), render(control.drawing()));
        let state = api::dispatch(&mut s, api::Request::State {}, (800, 600)).unwrap();
        let control_state =
            api::dispatch(&mut control, api::Request::State {}, (800, 600)).unwrap();
        assert_eq!(
            state["selectable_objects"],
            control_state["selectable_objects"]
        );
        assert_eq!(state["entities"], control_state["entities"]);
        let editor = acad_cmd::Editor::new(exploded.clone());
        assert_eq!(editor.pick_entity_at(Point { x: 10.0, y: 10.0 }, 0.1), None);
        assert_eq!(editor.pick_entity_at(Point { x: 17.0, y: 17.0 }, 0.1), None);
        // DXF is live-only: the root erased record and the nested erased leaf are omitted.
        assert_eq!(
            acad_dxf::try_write(&exploded).unwrap(),
            acad_dxf::try_write(control.drawing()).unwrap()
        );
        // DWG retains the negative root record and nested negative leaf.
        // END saves the attached document in its own revision.
        assert!(s.command("END").unwrap());
        let reopened = Session::open(&path, &[]).unwrap();
        // Established codec canonicalization: a root `Entity::Repeat` reopens as
        // `Item::Repeat` (independent of B4); every sign/layer/field is unchanged.
        let canonical: Vec<Item> = exploded
            .items
            .iter()
            .cloned()
            .map(|item| match item {
                Item::Entity(Entity::Repeat(r)) => Item::Repeat(r),
                other => other,
            })
            .collect();
        assert_eq!(reopened.drawing().items, canonical);
        let (_, meta) = acad_dwg::header::parse_header(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(meta.version, version);
        // OOPS only restores this session's ERASE; it does not revive promoted copies.
        commands(&mut s, &["OOPS"]);
        assert_eq!(s.drawing(), &exploded);
        // Exactly one UNDO step removes the whole explode.
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing().items, before.items);
    }
}

#[test]
fn named_wblock_keeps_erased_line_load_insert_members_as_negative_root_records() {
    let root = Scratch::new("wblock");
    let mut d = acad_dxf::parse(b"LIMITS,1\r\n0,10,0,10\r\nBLOCK,1\r\n0,0\r\nLEAF\r\nLOAD,1\r\nALT\r\nPOINT,2\r\n100,100\r\nENDBLK,1\r\nBLOCK,1\r\n0,0\r\nB\r\nLINE,3\r\n0,0,1,1\r\nLOAD,4\r\nMISSING\r\nINSERT,5\r\n0,0,1,1,0\r\nLEAF\r\nTEXT,1\r\n2,2,1,0\r\nA\r\nENDBLK,1\r\n").unwrap();
    let Item::Block(b) = &mut d.items[1] else {
        panic!("block")
    };
    for member in &mut b.entities[..3] {
        erase(member);
    }
    let block = b.clone();
    let leaf = d.items[0].clone();
    for version in [Version::Ac12, Version::Ac140] {
        let path = root.0.join(format!("{version:?}-input.dwg"));
        std::fs::write(&path, acad_dwg::write_version(&d, version).unwrap()).unwrap();
        let mut s = Session::open(&path, &[]).unwrap();
        let before = s.drawing().clone();
        let export = root.0.join(format!("{version:?}-b.dwg"));
        commands(&mut s, &["WBLOCK", export.to_str().unwrap(), "B"]);
        assert_eq!(s.drawing(), &before);
        let bytes = std::fs::read(&export).unwrap();
        let written = acad_dwg::parse(&bytes).unwrap();
        // Exact items: the erased INSERT keeps its referenced definition for stored
        // fields; each erased member is a root negative record with its own layer.
        let mut expected = vec![leaf.clone()];
        for member in &block.entities {
            expected.push(match member {
                Entity::Erased(inner) => Item::Erased(inner.as_ref().clone()),
                live => Item::Entity(live.clone()),
            });
        }
        assert_eq!(written.items, expected);
        for (item, layer) in written.items[1..4].iter().zip([3, 4, 5]) {
            assert!(
                matches!(item, Item::Erased(Entity::OnLayer { layer: actual, .. }) if *actual == layer)
            );
        }
        let (_, meta) = acad_dwg::header::parse_header(&bytes).unwrap();
        assert_eq!(meta.entity_count, 8, "LEAF(4) + 3 negative + TEXT");
        // Physical signs: only the TEXT and LEAF records are live entity records.
        let live = acad_dwg::entity::read_entities(&bytes, &meta).unwrap();
        assert!(live
            .iter()
            .all(|e| !matches!(acad_cmd_bare(e), Entity::Line { .. })));
        assert!(live
            .iter()
            .any(|e| matches!(acad_cmd_bare(e), Entity::Text { .. })));
        // No resource effect: the TEXT renders exactly as when exported alone,
        // so neither the erased LOAD nor the erased INSERT's LOAD ALT executes.
        let mut alone = written.clone();
        alone.items = vec![Item::Entity(block.entities[3].clone())];
        assert!(!render(&alone).is_empty());
        assert_eq!(render(&written), render(&alone));
        let mut opened = Session::open(&export, &[]).unwrap();
        let state = api::dispatch(&mut opened, api::Request::State {}, (800, 600)).unwrap();
        assert_eq!(state["selectable_objects"], 1);
        // DXF live-only exchange omits the negative records but keeps the definition.
        let dxf = acad_dxf::parse(&acad_dxf::try_write(&written).unwrap()).unwrap();
        assert_eq!(
            dxf.items,
            vec![leaf.clone(), Item::Entity(block.entities[3].clone())]
        );
    }
}

fn acad_cmd_bare(mut entity: &Entity) -> &Entity {
    while let Entity::OnLayer { entity: inner, .. } = entity {
        entity = inner;
    }
    entity
}
