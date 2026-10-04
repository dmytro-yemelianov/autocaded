//! Native policy recovery for retained partial physical-record erasure.
use acad_app::{api, Session};
use acad_model::{Entity, Item};
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "acad-partial-erased-{}-{label}",
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
fn source() -> acad_model::Drawing {
    let mut d = acad_dxf::parse(b"LIMITS,1\r\n0,10,0,10\r\nDWGVIEW,1\r\n3,2,10\r\nREPEAT,1\r\nLINE,2\r\n0,0,0,4\r\nLINE,1\r\n1,0,5,0\r\nENDREP,1\r\n1,1,0,0\r\n").unwrap();
    let Item::Repeat(r) = &mut d.items[0] else {
        panic!("group")
    };
    r.entities[0] = Entity::Erased(Box::new(r.entities[0].clone()));
    d.header.extents = acad_model::Extents {
        xmin: 1.0,
        xmax: 5.0,
        ymin: -0.5,
        ymax: 0.5,
    };
    d
}
fn commands(s: &mut Session, inputs: &[&str]) {
    for input in inputs {
        assert!(!s.command(input).unwrap());
    }
}
#[test]
fn erased_partial_owner_save_end_fail_atomically_and_oops_undo_keep_prior_members() {
    let root = Scratch::new("recover");
    for (index, version) in [
        acad_dwg::header::Version::Ac12,
        acad_dwg::header::Version::Ac140,
    ]
    .into_iter()
    .enumerate()
    {
        let path = root.0.join(format!("input-{index}.dwg"));
        let existing = root.0.join(format!("existing-{index}.dwg"));
        let absent = root.0.join(format!("absent-{index}.dwg"));
        let bytes = acad_dwg::write_version(&source(), version).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        std::fs::write(&existing, &bytes).unwrap();
        let mut s = Session::open(&path, &[]).unwrap();
        let live = s.drawing().clone();
        commands(&mut s, &["ERASE", "1", "POINT"]);
        let erased = s.drawing().clone();
        assert!(
            matches!(&erased.items[0], Item::Erased(Entity::Repeat(r)) if r.entities[0].is_erased())
        );
        let prompt = s.prompt().to_owned();
        s.set_input("pending point".into());
        let files = std::fs::read_dir(&root.0).unwrap().count();
        for target in [&existing, &absent] {
            assert!(api::dispatch(
                &mut s,
                api::Request::Save {
                    path: target.to_str().unwrap().into()
                },
                (800, 600)
            )
            .unwrap_err()
            .contains("already erased members"));
            assert_eq!(s.prompt(), prompt);
            assert_eq!(s.input(), "pending point");
            assert_eq!(s.drawing(), &erased);
            assert!(s.is_dirty());
            assert_eq!(s.document_path(), Some(path.as_path()));
        }
        assert_eq!(std::fs::read(&existing).unwrap(), bytes);
        assert!(!absent.exists());
        assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), files);
        // Exchange has its established live-only contract, including omission of the whole owner.
        let exchange = api::dispatch(&mut s, api::Request::Drawing {}, (800, 600)).unwrap();
        assert!(!exchange["data"].as_str().unwrap().contains("REPEAT"));
        assert_eq!(s.prompt(), prompt);
        assert_eq!(s.input(), "pending point");
        assert_eq!(s.drawing(), &erased);
        s.cancel().unwrap();
        assert!(s
            .command("END")
            .unwrap_err()
            .contains("already erased members"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(s.drawing(), &erased);
        assert!(s.is_dirty());
        commands(&mut s, &["OOPS"]);
        assert_eq!(s.drawing(), &live);
        assert!(s.command("END").unwrap());
        assert!(!s.is_dirty());
        let reopened = Session::open(&path, &[]).unwrap();
        assert_eq!(reopened.drawing().items, live.items);
        assert_eq!(
            acad_dwg::header::parse_header(&std::fs::read(&path).unwrap())
                .unwrap()
                .1
                .version,
            version
        );
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &erased);
        commands(&mut s, &["UNDO"]);
        assert_eq!(s.drawing(), &live);
        assert!(s.command("END").unwrap());
        assert_eq!(
            Session::open(&path, &[]).unwrap().drawing().items,
            live.items
        );
        // Partial signs survive reopen, but editor restoration history is session-local.
        let mut reopened = Session::open(&path, &[]).unwrap();
        let opened = reopened.drawing().clone();
        commands(&mut reopened, &["OOPS", "UNDO"]);
        assert_eq!(reopened.drawing(), &opened);
        let state = api::dispatch(&mut reopened, api::Request::State {}, (800, 600)).unwrap();
        assert_eq!(state["selectable_objects"], 1);
        assert_eq!(state["entities"], 1);
        let exchange = api::dispatch(&mut reopened, api::Request::Drawing {}, (800, 600)).unwrap();
        let exported = acad_dxf::parse(exchange["data"].as_str().unwrap().as_bytes()).unwrap();
        let Item::Repeat(r) = &exported.items[0] else {
            panic!("live exchange group")
        };
        assert_eq!(r.entities.len(), 1);
        assert_eq!(
            r.entities[0],
            match &live.items[0] {
                Item::Repeat(r) => r.entities[1].clone(),
                _ => unreachable!(),
            }
        );
        assert_eq!(reopened.drawing(), &opened);
    }
}
#[test]
fn block_partial_owner_source_refuses_full_save_but_named_export_preserves_live_block_and_dependencies(
) {
    let root = Scratch::new("block");
    let mut d = source();
    // A negative INSERT still needs its referenced definition for lossless stored fields,
    // without executing that definition's LOAD or geometry.
    let dependency = acad_dxf::parse(
        b"BLOCK,1\r\n0,0\r\nLEAF\r\nLOAD,1\r\nALT\r\nPOINT,2\r\n100,100\r\nENDBLK,1\r\n",
    )
    .unwrap()
    .items
    .remove(0);
    d.items.insert(0, dependency);
    let Item::Repeat(r) = &mut d.items[1] else {
        panic!("group")
    };
    r.entities.push(Entity::Erased(Box::new(Entity::OnLayer {
        layer: 3,
        entity: Box::new(Entity::Insert {
            name: "LEAF".into(),
            origin: acad_model::Point { x: 0.0, y: 0.0 },
            x_scale: 1.0,
            y_scale: 1.0,
            rotation_deg: 0.0,
        }),
    })));
    let path = root.0.join("source.dwg");
    let bytes = acad_dwg::write(&d).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let mut s = Session::open(&path, &[]).unwrap();
    let live = s.drawing().clone();
    commands(&mut s, &["BLOCK", "GROUP", "0,0", "1"]);
    let blocked = s.drawing().clone();
    assert!(s
        .save(&path)
        .unwrap_err()
        .contains("already erased members"));
    assert!(s
        .command("END")
        .unwrap_err()
        .contains("already erased members"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(s.drawing(), &blocked);
    assert!(s.is_dirty());
    let exported = root.0.join("group.dwg");
    commands(&mut s, &["WBLOCK", exported.to_str().unwrap(), "GROUP"]);
    let written = Session::open(&exported, &[]).unwrap();
    assert_eq!(written.drawing().block("LEAF"), live.block("LEAF"));
    let Item::Repeat(original) = &live.items[1] else {
        panic!("source group")
    };
    assert_eq!(
        written.drawing().items.last(),
        Some(&Item::Repeat(original.clone()))
    );
    assert_eq!(s.drawing(), &blocked);
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &live);
    s.save(&path).unwrap();
    assert_eq!(
        Session::open(&path, &[]).unwrap().drawing().items,
        live.items
    );
    // Erasing a root INSERT does not erase or reinterpret its independent block definition.
    commands(
        &mut s,
        &["INSERT", "LEAF", "0,0", "1", "1", "0", "ERASE", "2"],
    );
    let with_erased_insert = s.drawing().clone();
    s.save(&path).unwrap();
    assert_eq!(
        Session::open(&path, &[]).unwrap().drawing().items,
        with_erased_insert.items
    );
    assert_eq!(s.drawing().block("LEAF"), live.block("LEAF"));
}

#[test]
fn named_export_ignores_preceding_erased_load_and_insert_resource_effects() {
    let root = Scratch::new("context");
    // HIDDEN is a LOAD-bearing definition: executing an INSERT of it would project LOAD ALT.
    let source = b"BLOCK,1\r\n0,0\r\nHIDDEN\r\nLOAD,1\r\nALT\r\nPOINT,1\r\n5,5\r\nENDBLK,1\r\nREPEAT,1\r\nLOAD,2\r\nMISSING\r\nINSERT,2\r\n0,0,1,1,0\r\nHIDDEN\r\nPOINT,1\r\n100,100\r\nENDREP,1\r\n65535,65535,1,1\r\nBLOCK,1\r\n0,0\r\nGROUP\r\nTEXT,1\r\n0,0,1,0\r\nA\r\nENDBLK,1\r\n";
    let live = acad_dxf::parse(source).unwrap();
    let text = Item::Entity(live.block("GROUP").unwrap().entities[0].clone());
    let alt = |d: &acad_model::Drawing| d.block("HIDDEN").unwrap().entities[0].clone();
    for erased_members in [&[0usize, 1][..], &[0][..]] {
        let mut d = live.clone();
        let Item::Repeat(r) = &mut d.items[1] else {
            panic!("group")
        };
        for &index in erased_members {
            r.entities[index] = Entity::Erased(Box::new(r.entities[index].clone()));
        }
        let path = root.0.join(format!("input-{}.dwg", erased_members.len()));
        std::fs::write(&path, acad_dwg::write(&d).unwrap()).unwrap();
        let mut s = Session::open(&path, &[]).unwrap();
        let before = s.drawing().clone();
        assert_eq!(before.items, d.items);
        let export = root.0.join(format!("group-{}.dwg", erased_members.len()));
        commands(&mut s, &["WBLOCK", export.to_str().unwrap(), "GROUP"]);
        let reopened = Session::open(&export, &[]).unwrap();
        let expected = if erased_members.len() == 2 {
            // Neither the erased LOAD nor the erased INSERT's definition body leaks a LOAD.
            vec![text.clone()]
        } else {
            // Control: the same INSERT live does project its definition's ordered LOAD.
            vec![Item::Entity(alt(&live)), text.clone()]
        };
        assert_eq!(reopened.drawing().items, expected);
        assert_eq!(s.drawing(), &before);
        assert!(!s.is_dirty());
    }
}
