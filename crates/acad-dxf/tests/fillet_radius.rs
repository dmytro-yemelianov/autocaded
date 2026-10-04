//! Historical DXF has no audited radius token; zero default is native policy.
#[test]
fn zero_import_export_and_checked_nonzero_invalid_refusal() {
    let mut d = acad_dxf::parse(b"\x1a").unwrap();
    assert_eq!(d.header.fillet_radius, 0.0);
    let bytes = acad_dxf::try_write(&d).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("FILLET"));
    assert_eq!(acad_dxf::parse(&bytes).unwrap().header.fillet_radius, 0.0);
    for value in [2.5, -1.0, f64::NAN, f64::INFINITY] {
        d.header.fillet_radius = value;
        assert_eq!(
            acad_dxf::try_write(&d).unwrap_err(),
            acad_dxf::DxfError::UnsupportedFilletRadius
        );
    }
}
