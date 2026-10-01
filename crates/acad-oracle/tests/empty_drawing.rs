#[cfg(unix)]
#[test]
fn original_empty_drawing_round_trips_through_our_dxf_codec() {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if !disk.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
        return;
    }
    let (dwg, dxf) = acad_oracle::generate_pair(&disk, "ORCEMPTY", &[]).unwrap();
    assert_eq!(&dwg[..6], b"AC1.40");
    assert_eq!(dwg.len(), 640, "empty drawing emitted by the 1983 writer");
    let drawing = acad_dxf::parse(&dxf).unwrap();
    assert_eq!(drawing.entities().count(), 0);
    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(
        acad_dxf::write(&drawing),
        dxf[..end],
        "Rust DXF output must match the original's generated interchange text"
    );
    assert_eq!(acad_dxf::write(&acad_dwg::parse(&dwg).unwrap()), dxf[..end]);

    // A fresh editor must start with the original's recovered drawing
    // limits and saved view, before any command can hide a mismatch.
    let native = acad_dwg::parse(&dwg).unwrap();
    let mut rust = acad_cmd::Editor::default();
    assert_eq!(
        native.header.limits,
        acad_model::Extents {
            xmin: 0.0,
            ymin: 0.0,
            xmax: 12.0,
            ymax: 9.0,
        }
    );
    assert_eq!(native.header.view.height, 9.0);
    assert_eq!(native.header.view.center.y, 4.5);
    assert!((native.header.view.center.x - 6.850_490_196_078_431).abs() < 1e-12);
    assert_eq!(rust.drawing().header.limits, native.header.limits);
    let assert_view = |view: acad_model::DwgView| {
        assert!((view.center.x - native.header.view.center.x).abs() < 1e-12);
        assert_eq!(view.center.y, native.header.view.center.y);
        assert_eq!(view.height, native.header.view.height);
    };
    assert_view(rust.drawing().header.view);
    rust.submit("ZOOM").unwrap();
    rust.submit("ALL").unwrap();
    assert_view(rust.drawing().header.view);
    let zoomed_dwg = acad_oracle::generate_dwg(&disk, "ORCNEWZA", &["ZOOM", "A"]).unwrap();
    assert_view(acad_dwg::parse(&zoomed_dwg).unwrap().header.view);
}
