use acad_re::analysis::CallGraph;
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

#[test]
fn call_graph_links_caller_to_callee() {
    // Varnode offsets are linear addresses: 0x1000 * 16 + 0x01a3 = 65955.
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"main","block":"EXE_CODE","markers":[],
       "ops":[{"seq":"1000:0011","op":"CALL","out":null,
               "in":[{"space":"ram","offset":65955,"size":2,"unique":false}]}]},
      {"address":"1000:01a3","name":"open_ovl","block":"EXE_CODE","markers":[],"ops":[]}
    ],"failures":[]}"#;
    let g = CallGraph::from_pcode(&PcodeExport::from_json(json).unwrap());
    assert_eq!(g.callees_of("1000:0003"), vec!["1000:01a3"]);
    assert_eq!(g.callers_of("1000:01a3"), vec!["1000:0003"]);
}

#[test]
fn a_call_to_an_unexported_address_is_dropped_not_invented() {
    // A CALL whose target is not a function we exported must not create a node,
    // or the graph grows phantom vertices that no analysis can explain.
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"main","block":"EXE_CODE","markers":[],
       "ops":[{"seq":"1000:0011","op":"CALL","out":null,
               "in":[{"space":"ram","offset":9999999,"size":2,"unique":false}]}]}
    ],"failures":[]}"#;
    let g = CallGraph::from_pcode(&PcodeExport::from_json(json).unwrap());
    assert!(g.callees_of("1000:0003").is_empty());
}

#[test]
fn cross_overlay_edges_are_identified_by_block() {
    // 0x2321 * 16 + 0x0200 = 144400.
    let json = r#"{"functions":[
      {"address":"2321:0100","name":"a","block":"OVL00_CODE","markers":[],
       "ops":[{"seq":"2321:0110","op":"CALL","out":null,
               "in":[{"space":"ram","offset":144400,"size":2,"unique":false}]}]},
      {"address":"2321:0200","name":"b","block":"OVL01_CODE","markers":[],"ops":[]}
    ],"failures":[]}"#;
    let g = CallGraph::from_pcode(&PcodeExport::from_json(json).unwrap());
    assert_eq!(g.cross_overlay_edges(), vec![("2321:0100", "2321:0200")]);
}
