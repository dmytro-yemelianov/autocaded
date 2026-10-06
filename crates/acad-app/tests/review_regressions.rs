use acad_app::Session;
use acad_model::{Entity, Item, Point};

#[test]
fn pixel_pan_moves_ink_with_the_pointer_and_supports_previous() {
    let mut session = Session::default();
    for input in ["ZOOM", "C", "0,0", "10"] {
        session.command(input).unwrap();
    }
    let before = session.drawing().header.view;
    // 244 client pixels leave a 200-pixel drawing canvas: 20 pixels/unit.
    session.pan_pixels(40.0, 60.0, 400, 244).unwrap();
    assert_eq!(
        session.drawing().header.view.center,
        Point { x: -2.0, y: 3.0 }
    );
    assert_eq!(session.drawing().header.view.height, before.height);
    assert_eq!(session.drawing().entities().count(), 0);
    session.command("ZOOM").unwrap();
    session.command("P").unwrap();
    assert_eq!(session.drawing().header.view, before);
}

#[test]
fn pixel_pan_preserves_pending_input_and_rejects_invalid_displacements() {
    let mut session = Session::default();
    let before = session.drawing().clone();
    session.set_input("LI".into());
    session.pan_pixels(40.0, 60.0, 400, 244).unwrap();
    assert_eq!(session.input(), "LI");
    assert_eq!(session.drawing(), &before);
    session.set_input(String::new());
    assert!(session.pan_pixels(f64::NAN, 0.0, 400, 244).is_err());
    assert!(session.pan_pixels(f64::MAX, 0.0, 400, 244).is_err());
    assert_eq!(session.prompt(), "Command");
    session.command("LINE").unwrap();
    let prompt = session.prompt().to_owned();
    session.pan_pixels(40.0, 60.0, 400, 244).unwrap();
    assert_eq!(session.prompt(), prompt);
    assert_eq!(session.drawing(), &before);
}

#[test]
fn rejected_dxf_save_preserves_the_file_and_attached_dwg() {
    let directory = std::env::temp_dir().join(format!("acad-dxf-controls-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let source = directory.join("source.dwg");
    let destination = directory.join("drawing.dxf");
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,1\r\n").unwrap();
    drawing.items = vec![Item::Entity(Entity::Text {
        origin: Point { x: 0.0, y: 0.0 },
        height: 1.0,
        rotation_deg: 0.0,
        value: "first\r\nsecond".into(),
    })];
    std::fs::write(&source, acad_dwg::write(&drawing).unwrap()).unwrap();
    std::fs::write(&destination, b"previous drawing").unwrap();
    let mut session = Session::open(&source, &[]).unwrap();
    let before = session.drawing().clone();
    assert!(session
        .save(&destination)
        .unwrap_err()
        .contains("TEXT value"));
    assert_eq!(std::fs::read(&destination).unwrap(), b"previous drawing");
    assert!(!destination.with_extension("bak").exists());
    assert_eq!(session.document_path(), Some(source.as_path()));
    assert_eq!(session.drawing(), &before);
    assert!(!session.is_dirty());
}
