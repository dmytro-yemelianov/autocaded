//! Synthetic static-contract fixtures: signed table words verified by retained
//! program bytes; these are not measured original OFF export/reopen fixtures.
use acad_dwg::{header::Version, DwgError};
use acad_model::Drawing;
fn drawing() -> Drawing {
    let mut d = acad_dxf::parse(b"POINT,1\r\n1,2\r\n").unwrap();
    d.header.current_layer = 7;
    d.header.layers = [(0, 0), (1, 1), (7, 7), (64, 64), (127, 127), (2, 254)]
        .into_iter()
        .collect();
    d.header.off_layers = [1, 7, 64, 127].into_iter().collect();
    d
}
#[test]
fn signed_off_bytes_and_reopen_preserve_colors_both_revisions() {
    for version in [Version::Ac12, Version::Ac140] {
        let d = drawing();
        let bytes = acad_dwg::write_version(&d, version).unwrap();
        for (layer, expected) in [
            (0, [0, 0]),
            (1, [0xff, 0xff]),
            (7, [0xf9, 0xff]),
            (64, [0xc0, 0xff]),
            (127, [0x81, 0xff]),
            (2, [254, 0]),
            (3, [255, 0]),
        ] {
            assert_eq!(
                &bytes[0xc8 + 2 * layer..0xca + 2 * layer],
                &expected,
                "layer {layer}"
            );
        }
        let reopened = acad_dwg::parse(&bytes).unwrap();
        assert_eq!(reopened.header.layers, d.header.layers);
        assert_eq!(reopened.header.off_layers, d.header.off_layers);
        assert_eq!(reopened.header.current_layer, 7);
        assert_eq!(reopened.items, d.items);
        assert!(!reopened.header.layer_is_visible(7));
        assert!(reopened.header.layer_is_visible(2));
        assert_eq!(acad_dwg::write_version(&reopened, version).unwrap(), bytes);
    }
}
#[test]
fn positive_table_range_and_unused_sentinel_remain_distinct() {
    for version in [Version::Ac12, Version::Ac140] {
        for color in 0..=254 {
            let mut d = drawing();
            d.header.off_layers.clear();
            d.header.layers.insert(7, color);
            let bytes = acad_dwg::write_version(&d, version).unwrap();
            let reopened = acad_dwg::parse(&bytes).unwrap();
            assert_eq!(reopened.header.layers[&7], color);
            assert!(!reopened.header.layers.contains_key(&3));
            assert!(reopened.header.off_layers.is_empty());
        }
    }
}
#[test]
fn malformed_signed_magnitudes_and_off_layer_zero_fail_without_abs_overflow() {
    for version in [Version::Ac12, Version::Ac140] {
        let bytes = acad_dwg::write_version(&drawing(), version).unwrap();
        for (layer, value) in [(0, -7i16), (7, -128), (7, -254), (7, -255), (7, i16::MIN)] {
            let mut invalid = bytes.clone();
            invalid[0xc8 + 2 * layer..0xca + 2 * layer].copy_from_slice(&value.to_le_bytes());
            assert_eq!(
                acad_dwg::parse(&invalid).unwrap_err(),
                DwgError::InvalidHeaderValue {
                    field: "OFF layer color",
                    value: value as u16
                }
            );
        }
    }
}
#[test]
fn every_writer_checks_unrepresentable_off_state() {
    for (layer, color) in [
        (0, Some(7)),
        (1, Some(0)),
        (1, Some(128)),
        (1, Some(254)),
        (3, None),
        (128, None),
        (255, None),
    ] {
        let mut d = drawing();
        d.header.off_layers.clear();
        d.header.off_layers.insert(layer);
        if let Some(color) = color {
            d.header.layers.insert(layer, color);
        }
        let errors = [
            acad_dwg::write(&d),
            acad_dwg::write::encode(&d),
            acad_dwg::write_version(&d, Version::Ac12),
            acad_dwg::write::encode_version(&d, Version::Ac140),
        ];
        for result in errors {
            assert!(matches!(
                result.unwrap_err(),
                DwgError::WriteValue {
                    field: "layer visibility",
                    ..
                }
            ));
        }
    }
}

#[test]
fn retained_positive_corpus_layer_words_survive_header_reencoding() {
    // Retained positive headers only: this checks the changed layer-table path,
    // not original OFF measurements or unrelated record-stream roundtrip parity.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/Samples");
    if !root.exists() {
        eprintln!("skipping: corpus absent");
        return;
    }
    let files = [
        "ADDER.DWG",
        "ANDGATE.DWG",
        "BLIVET.DWG",
        "BOX.DWG",
        "DLATCH.DWG",
        "FLOOR.DWG",
        "FLOW.DWG",
        "HALFADD.DWG",
        "INVERTER.DWG",
        "NANDGATE.DWG",
        "NORGATE.DWG",
        "ORGATE.DWG",
        "SELEXOL.DWG",
        "SUBDIV.DWG",
        "XNORGATE.DWG",
        "XORGATE.DWG",
        "HOUSE.DWG",
        "HOUSE.BAK",
        "COLORS.DWG",
        "COLORS.BAK",
        "OFFICE.DWG",
        "OFFICE.BAK",
        "SHUTTLE.DWG",
        "DISC.BAK",
    ];
    assert_eq!(files.len(), 24);
    for name in files {
        let original = std::fs::read(root.join(name)).unwrap();
        let (header, meta) = acad_dwg::header::parse_header(&original).unwrap();
        assert!(
            header.off_layers.is_empty(),
            "{name}: retained corpus has positive layer tables"
        );
        let header_only = Drawing {
            header,
            items: Vec::new(),
        };
        let encoded = acad_dwg::write_version(&header_only, meta.version).unwrap();
        assert_eq!(
            &encoded[0xc8..0x1c8],
            &original[0xc8..0x1c8],
            "{name}: exact existing positive/sentinel words"
        );
        let reopened = acad_dwg::parse(&encoded).unwrap();
        assert_eq!(reopened.header.layers, header_only.header.layers);
        assert!(reopened.header.off_layers.is_empty());
    }
}
