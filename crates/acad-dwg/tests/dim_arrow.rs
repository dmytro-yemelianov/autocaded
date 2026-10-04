use acad_dwg::{
    header::{parse_header, Version},
    parse, write, write_version, DwgError,
};
const DEFAULT: &[u8] = include_bytes!("../../acad-cmd/tests/fixtures/dim/DPRIOR.dwg");

#[test]
fn every_unidentified_header_byte_survives_distinguishable_sentinels() {
    for (version, end) in [(Version::Ac140, 0x202), (Version::Ac12, 0x1d8)] {
        let mut original = DEFAULT[..end].to_vec();
        if version == Version::Ac12 {
            original[..7].copy_from_slice(b"AC1.2\0\0");
        }
        // Independently specified identified byte ranges; bytes outside these
        // get nonzero sentinels so resetting opaque zero-filled data cannot pass.
        let common = [
            0x0c..0x1c,
            0x24..0x3a,
            0x42..0x52,
            0x5a..0x8a,
            0x92..0xb0,
            0xb2..0xc6,
            0xc8..0x1c8,
        ];
        for (at, byte) in original.iter_mut().enumerate() {
            let magic_end = if version == Version::Ac140 { 7 } else { 6 };
            let known = at < magic_end
                || common.iter().any(|r| r.contains(&at))
                || (version == Version::Ac140
                    && [0x1c8..0x1d0, 0x1d8..0x1dc, 0x1e0..0x1ea, 0x1fa..0x202]
                        .iter()
                        .any(|r| r.contains(&at)));
            if !known {
                *byte = ((at * 37) % 254 + 1) as u8;
            }
        }
        let (header, _) = parse_header(&original).unwrap();
        let drawing = acad_model::Drawing {
            header,
            items: vec![],
        };
        let output = write_version(&drawing, version).unwrap();
        let mut expected = original;
        expected[0x24..0x28].copy_from_slice(&(end as u32).to_le_bytes());
        expected[0x28..0x2a].fill(0);
        assert_eq!(&output[..end], expected, "{version} complete fixed header");
    }
}
#[test]
fn observed_ac140_dim_arrow_mapping_and_mutation_preserve_other_header_bytes() {
    for (bytes, value) in [
        (DEFAULT, 0.140625),
        (
            include_bytes!("../../acad-cmd/tests/fixtures/dim/DAR025.dwg").as_slice(),
            0.25,
        ),
        (
            include_bytes!("../../acad-cmd/tests/fixtures/dim/DAR050.dwg").as_slice(),
            0.5,
        ),
        (
            include_bytes!("../../acad-cmd/tests/fixtures/dim/DAR200.dwg").as_slice(),
            2.,
        ),
    ] {
        let mut d = parse(bytes).unwrap();
        assert_eq!(d.header.dim_arrow, Some(value));
        assert_eq!(&write(&d).unwrap()[..0x202], &bytes[..0x202]);
        d.header.dim_arrow = Some(0.75);
        let actual = write(&d).unwrap();
        let mut expected = bytes[..0x202].to_vec();
        expected[0x1c8..0x1d0].copy_from_slice(&0.75f64.to_le_bytes());
        assert_eq!(&actual[..0x202], expected);
        assert_eq!(parse(&actual).unwrap().header.dim_arrow, Some(0.75));
    }
}
#[test]
fn positive_zero_absence_negative_zero_and_cleared_passthrough_boundaries() {
    let mut d = parse(DEFAULT).unwrap();
    d.header.dim_arrow = None;
    let bytes = write(&d).unwrap();
    assert_eq!(&bytes[..0x202], &DEFAULT[..0x202]);
    assert_eq!(parse(&bytes).unwrap().header.dim_arrow, Some(0.140625));
    d.header.dwg_header_passthrough = None;
    let bytes = write(&d).unwrap();
    assert_eq!(&bytes[0x1c8..0x1d0], &[0; 8]);
    assert_eq!(parse(&bytes).unwrap().header.dim_arrow, None);
    d.header.dim_arrow = Some(0.);
    let bytes = write(&d).unwrap();
    assert_eq!(parse(&bytes).unwrap().header.dim_arrow, None); // explicit presence loss
    d.header.dim_arrow = Some(-0.);
    let bytes = write(&d).unwrap();
    assert_eq!(&bytes[0x1c8..0x1d0], &(1u64 << 63).to_le_bytes());
    assert_eq!(
        parse(&bytes).unwrap().header.dim_arrow.unwrap().to_bits(),
        1u64 << 63
    );
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        d.header.dim_arrow = Some(value);
        assert!(matches!(
            write(&d),
            Err(DwgError::WriteValue {
                field: "DIMARROW",
                ..
            })
        ));
    }
}
#[test]
fn ac12_retains_unmapped_slot_and_rejects_explicit_field() {
    // Independently modify only a native fixed header's revision magic. Header
    // decoding needs no entity layout conversion or invented native A behavior.
    let mut original = DEFAULT[..0x1d8].to_vec();
    original[..7].copy_from_slice(b"AC1.2\0\0");
    original[0x1c8..0x1d0].copy_from_slice(&[0x42, 0x19, 0x73, 0xa5, 0x11, 0x27, 0x94, 0x63]);
    let (h, meta) = parse_header(&original).unwrap();
    assert_eq!(meta.version, Version::Ac12);
    assert_eq!(h.dim_arrow, None);
    let mut d = acad_model::Drawing {
        header: h,
        items: vec![],
    };
    let bytes = write_version(&d, Version::Ac12).unwrap();
    // Mechanical entity end/count change because this test has no entities.
    let mut expected = original.clone();
    expected[0x24..0x28].copy_from_slice(&0x1d8u32.to_le_bytes());
    expected[0x28..0x2a].fill(0);
    assert_eq!(&bytes[..0x1d8], expected);
    d.header.dim_arrow = Some(0.25);
    assert!(matches!(
        write_version(&d, Version::Ac12),
        Err(DwgError::WriteValue {
            field: "DIMARROW",
            ..
        })
    ));
}
