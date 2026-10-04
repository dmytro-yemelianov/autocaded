use acad_dxf::{parse, write, DxfError};
const DEFAULT: &[u8] = include_bytes!("../../acad-cmd/tests/fixtures/dim/DPRIOR.dxf");
fn arrow(record: &str) -> Vec<u8> {
    let eof = DEFAULT.iter().position(|b| *b == 0x1a).unwrap();
    String::from_utf8(DEFAULT[..=eof].to_vec())
        .unwrap()
        .replace("DIMARROW,1\r\n0.140625\r\n", record)
        .into_bytes()
}
#[test]
fn observed_dim_arrow_values_roundtrip_in_native_record_order() {
    for (bytes, expected) in [
        (DEFAULT, 0.140625),
        (
            include_bytes!("../../acad-cmd/tests/fixtures/dim/DAR025.dxf").as_slice(),
            0.25,
        ),
        (
            include_bytes!("../../acad-cmd/tests/fixtures/dim/DAR050.dxf").as_slice(),
            0.5,
        ),
        (
            include_bytes!("../../acad-cmd/tests/fixtures/dim/DAR200.dxf").as_slice(),
            2.0,
        ),
    ] {
        let d = parse(bytes).unwrap();
        assert_eq!(d.header.dim_arrow, Some(expected));
        let eof = bytes.iter().position(|b| *b == 0x1a).unwrap();
        assert_eq!(write(&d), &bytes[..=eof]);
        let records = acad_dxf::lex(bytes).unwrap();
        let index = records
            .iter()
            .position(|r| r.keyword == "DIMARROW")
            .unwrap();
        assert_eq!(records[index - 1].keyword, "DWGVIEW");
        assert_eq!(records[index + 1].keyword, "MODERES");
        assert_eq!(records[index].suffix, 1);
        assert_eq!(records[index].rows.len(), 1);
    }
}
#[test]
fn absent_and_explicit_signed_zero_presence_is_preserved() {
    const OMITTED: &[u8] = include_bytes!(
        "../../../docs/recovery/2026-10-01-menu-controls/controls/drawings/CMENU.dxf"
    );
    let d = parse(OMITTED).unwrap();
    assert_eq!(d.header.dim_arrow, None);
    let eof = OMITTED.iter().position(|b| *b == 0x1a).unwrap();
    assert_eq!(write(&d), &OMITTED[..=eof]);
    for (token, bits) in [("0.000000", 0), ("-0.000000", 1u64 << 63)] {
        let bytes = arrow(&format!("DIMARROW,1\r\n{token}\r\n"));
        let d = parse(&bytes).unwrap();
        assert_eq!(d.header.dim_arrow.unwrap().to_bits(), bits);
        assert_eq!(write(&d), bytes);
    }
}
#[test]
fn dim_arrow_rejects_invalid_arity_numbers_and_unknown_keywords() {
    for token in ["", "abc", "NaN", "inf", "-inf", "1e999", "0.25,0.5"] {
        assert!(
            parse(&arrow(&format!("DIMARROW,1\r\n{token}\r\n"))).is_err(),
            "{token}"
        );
    }
    for suffix in ["0", "2", "-1", "garbage"] {
        assert!(
            parse(&arrow(&format!("DIMARROW,{suffix}\r\n0.250000\r\n"))).is_err(),
            "{suffix}"
        );
    }
    assert!(matches!(
        parse(&arrow("DIMUNKNOWN,1\r\n0.250000\r\n")),
        Err(DxfError::UnknownKeyword { .. })
    ));
}
