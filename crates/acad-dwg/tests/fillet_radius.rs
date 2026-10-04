//! Synthetic checks of the independently audited descriptor42/AC1.40 mapping.
use acad_dwg::{header::Version, DwgError};
fn empty() -> acad_model::Drawing {
    let mut bytes = vec![0; 0x202];
    bytes[..7].copy_from_slice(b"AC1.40\0");
    bytes[0x24..0x28].copy_from_slice(&0x202_u32.to_le_bytes());
    acad_dwg::parse(&bytes).unwrap()
}
#[test]
fn radius_exact_ac140_bytes_reopen_and_passthrough_overwrite() {
    let mut d = empty();
    d.header.fillet_radius = 2.5;
    let bytes = acad_dwg::write(&d).unwrap();
    assert_eq!(&bytes[0x1fa..0x202], &[0, 0, 0, 0, 0, 0, 4, 64]);
    let mut parsed = acad_dwg::parse(&bytes).unwrap();
    assert_eq!(parsed.header.fillet_radius, 2.5);
    assert_eq!(acad_dwg::write(&parsed).unwrap(), bytes);
    parsed.header.fillet_radius = 0.75;
    let output = acad_dwg::write(&parsed).unwrap();
    assert_eq!(&output[0x1fa..0x202], &0.75_f64.to_le_bytes());
    assert_eq!(&output[..0x1fa], &bytes[..0x1fa]);
    parsed.header.fillet_radius = 0.0;
    assert_eq!(&acad_dwg::write(&parsed).unwrap()[0x1fa..0x202], &[0; 8]);
}
#[test]
fn ac12_omits_zero_and_all_writers_refuse_nonzero_or_invalid_radius() {
    let mut d = empty();
    let bytes = acad_dwg::write_version(&d, Version::Ac12).unwrap();
    assert_eq!(acad_dwg::parse(&bytes).unwrap().header.fillet_radius, 0.0);
    assert_eq!(
        u32::from_le_bytes(bytes[0x24..0x28].try_into().unwrap()),
        0x1d8
    );
    d.header.fillet_radius = 2.5;
    assert!(acad_dwg::write_version(&d, Version::Ac12)
        .unwrap_err()
        .to_string()
        .contains("FILLET radius"));
    assert!(acad_dwg::write::encode_version(&d, Version::Ac12).is_err());
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        d.header.fillet_radius = invalid;
        for version in [Version::Ac12, Version::Ac140] {
            assert!(acad_dwg::write_version(&d, version).is_err());
            assert!(acad_dwg::write::encode_version(&d, version).is_err());
        }
        assert!(acad_dwg::write(&d).is_err());
        assert!(acad_dwg::write::encode(&d).is_err());
    }
}
#[test]
fn invalid_ac140_radius_is_a_checked_header_error() {
    let bytes = acad_dwg::write(&empty()).unwrap();
    for value in [-1.0, f64::NAN, f64::INFINITY] {
        let mut bad = bytes.clone();
        bad[0x1fa..0x202].copy_from_slice(&value.to_le_bytes());
        assert!(matches!(
            acad_dwg::parse(&bad),
            Err(DwgError::InvalidHeaderScalar {
                field: "FILLET radius",
                ..
            })
        ));
    }
}
#[test]
fn original_ac140_sample_defaults_remain_zero() {
    let Some(path) = corpus("Samples") else {
        return;
    };
    let mut count = 0;
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dwg") || e.eq_ignore_ascii_case("bak"))
        {
            continue;
        }
        let bytes = std::fs::read(path).unwrap();
        if bytes.starts_with(b"AC1.40") {
            count += 1;
            assert_eq!(&bytes[0x1fa..0x202], &[0; 8]);
            let d = acad_dwg::parse(&bytes).unwrap();
            assert_eq!(d.header.fillet_radius, 0.0);
            assert_eq!(&acad_dwg::write(&d).unwrap()[0x1fa..0x202], &[0; 8]);
        }
    }
    assert_eq!(count, 8, "retained original AC1.40 sample census");
}

/// `corpus/<path>` when the retained corpus is extracted. Otherwise the
/// caller skips visibly; `AUTOCAD_REQUIRE_CORPUS=1` makes absence a failure.
fn corpus(path: &str) -> Option<std::path::PathBuf> {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(path);
    if full.exists() {
        return Some(full);
    }
    let message = format!("corpus {} absent", full.display());
    assert!(
        std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
        "{message}"
    );
    eprintln!("skipping corpus test, NOT validated: {message}");
    None
}
