use acad_re::ast::{ClangExport, PcodeExport};

const PCODE: &str = include_str!("fixtures/ast-pcode-sample.json");
const CLANG: &str = include_str!("fixtures/ast-clang-sample.json");

#[test]
fn pcode_export_deserializes() {
    let e = PcodeExport::from_json(PCODE).unwrap();
    assert!(!e.functions.is_empty());
    let f = &e.functions[0];
    assert!(!f.address.is_empty(), "every function is keyed by address");
    assert!(!f.body.ops.is_empty(), "a clean function has P-Code ops");
}

#[test]
fn clang_export_deserializes() {
    let e = ClangExport::from_json(CLANG).unwrap();
    assert!(!e.functions.is_empty());
    assert!(!e.functions[0].body.nodes.is_empty());
}

#[test]
fn failures_survive_the_round_trip() {
    // Review Focus 5: a dropped failure inflates the gate's success rate.
    let e = PcodeExport::from_json(PCODE).unwrap();
    assert_eq!(e.failures.len(), 1);
    assert!(
        !e.failures[0].reason.is_empty(),
        "a failure always says why"
    );
}

#[test]
fn marked_functions_are_excluded_from_the_clean_set() {
    let e = PcodeExport::from_json(PCODE).unwrap();
    let clean: Vec<_> = e.decompiled_cleanly().collect();
    assert!(clean.iter().all(|f| f.markers.is_empty()));
    assert!(
        clean.len() < e.functions.len(),
        "the fixture includes a halt_baddata function, so some are excluded"
    );
}

#[test]
fn an_unknown_field_does_not_break_deserialization() {
    // The exporter will grow fields; older Rust must still read newer JSON.
    let json = r#"{"functions":[{"address":"1000:0003","name":"f","block":"EXE_CODE",
                   "markers":[],"ops":[],"future_field":42}],"failures":[]}"#;
    assert_eq!(PcodeExport::from_json(json).unwrap().functions.len(), 1);
}
