//! Exercise the committed pilots through the same Session used by the native UI.
use acad_app::{art::Recipe, Session};
use acad_model::Palette;
use std::path::Path;

#[test]
fn committed_pilots_open_edit_undo_and_keep_pending_input_across_presentation_changes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/art");
    let catalog: acad_app::art::Catalog =
        serde_json::from_slice(&std::fs::read(root.join("catalog.json")).unwrap()).unwrap();
    let mut libraries = acad_render::Libraries::default();
    libraries
        .insert("TXT", include_bytes!("../../../demo/AUTOCADED.SHP"))
        .unwrap();
    for entry in catalog.entries {
        let id = &entry.id;
        let recipe =
            Recipe::parse(&std::fs::read(root.join(format!("recipes/{id}.json"))).unwrap())
                .unwrap();
        let authored = recipe.compile_with_libraries(&libraries).unwrap();
        for ext in ["DWG", "DXF"] {
            let path = root.join(format!("generated/{id}.{ext}"));
            let mut session = Session::open(&path, &[]).unwrap();
            assert_eq!(session.drawing().items, authored.items);
            session.set_mode("modern").unwrap();
            assert_eq!(session.palette(), Palette::Aci256);
            for (width, height) in [(800, 600), (390, 640)] {
                let start = std::time::Instant::now();
                let frame = session.frame(width, height).unwrap();
                eprintln!("{id}.{ext} {width}x{height}: {:?}", start.elapsed());
                assert!(frame.complete);
                assert!(frame.diagnostics.is_empty(), "{:?}", frame.diagnostics);
                assert!(frame.pixels.iter().any(|p| *p != 0));
            }
            for input in ["MOVE", "1,0", "", "1"] {
                session.command(input).unwrap();
            }
            assert_ne!(
                session.drawing().items,
                authored.items,
                "ordinary entity moves"
            );
            session.command("UNDO").unwrap();
            assert_eq!(session.drawing().items, authored.items);
            session.command("LINE").unwrap();
            session.command("0,0").unwrap();
            session.set_input("1,2".into());
            let before = acad_dxf::try_write(session.drawing()).unwrap();
            session.set_locale_tag("uk").unwrap();
            session.set_mode("frozen").unwrap();
            session.set_mode("modern").unwrap();
            assert_eq!(session.input(), "1,2");
            assert_eq!(
                session.prompt(),
                "LINE: наступна точка (Enter для завершення)"
            );
            assert_eq!(acad_dxf::try_write(session.drawing()).unwrap(), before);
            session.cancel().unwrap();
            let reopened = acad_app::decode_drawing(
                &acad_dwg::write_version(session.drawing(), acad_dwg::header::Version::Ac140)
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(reopened.items, authored.items);
        }
    }
}
