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
}
