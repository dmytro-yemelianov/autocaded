use acad_re::analysis::{CallGraph, Gate};
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

#[test]
fn the_gate_counts_failures_against_the_total() {
    // Review Focus 5 again, at the level that decides the milestone: a function
    // that failed to decompile is neither clean nor absent — it is a denominator.
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"a","block":"EXE_CODE","markers":[],"ops":[]},
      {"address":"1000:0100","name":"b","block":"EXE_CODE","markers":["halt_baddata"],"ops":[]}
    ],"failures":[{"address":"1000:0200","name":"c","block":"EXE_CODE","reason":"timeout"}]}"#;
    let e = PcodeExport::from_json(json).unwrap();
    let g = Gate::measure(&e, &CallGraph::from_pcode(&e));
    assert_eq!(g.total, 3, "two exported plus one failed");
    assert_eq!(g.clean, 1);
    assert_eq!(g.marked, 1);
    assert_eq!(g.failed, 1);
    assert!((g.clean_ratio - 1.0 / 3.0).abs() < 1e-9);
}

#[test]
fn an_empty_export_does_not_divide_by_zero() {
    let e = PcodeExport::from_json(r#"{"functions":[],"failures":[]}"#).unwrap();
    let g = Gate::measure(&e, &CallGraph::from_pcode(&e));
    assert_eq!(g.total, 0);
    assert_eq!(g.clean_ratio, 0.0);
}

#[test]
fn overlay_addresses_resolve_within_their_own_space() {
    // Ghidra writes overlay addresses as "SPACE::linearhex", not "seg:off".
    // 0x2321 * 16 + 0x0100 = 0x23310 and + 0x0200 = 0x23410.
    let json = r#"{"functions":[
      {"address":"OVL00_CODE::023310","name":"a","block":"OVL00_CODE","markers":[],
       "ops":[{"seq":"OVL00_CODE::023312","op":"CALL","out":null,
               "in":[{"space":"ram","offset":144400,"size":2,"unique":false}]}]},
      {"address":"OVL00_CODE::023410","name":"b","block":"OVL00_CODE","markers":[],"ops":[]}
    ],"failures":[]}"#;
    let g = CallGraph::from_pcode(&PcodeExport::from_json(json).unwrap());
    assert_eq!(
        g.callees_of("OVL00_CODE::023310"),
        vec!["OVL00_CODE::023410"]
    );
}

#[test]
fn an_address_ambiguous_across_overlay_spaces_is_not_guessed() {
    // Overlay spaces share linear addresses, because the overlays share the
    // window. With two candidates and neither in the caller's space nor the
    // EXE's, picking one would invent an edge.
    let json = r#"{"functions":[
      {"address":"OVL00_CODE::023310","name":"a","block":"OVL00_CODE","markers":[],
       "ops":[{"seq":"OVL00_CODE::023312","op":"CALL","out":null,
               "in":[{"space":"ram","offset":144400,"size":2,"unique":false}]}]},
      {"address":"OVL01_CODE::023410","name":"b","block":"OVL01_CODE","markers":[],"ops":[]},
      {"address":"OVL02_CODE::023410","name":"c","block":"OVL02_CODE","markers":[],"ops":[]}
    ],"failures":[]}"#;
    let g = CallGraph::from_pcode(&PcodeExport::from_json(json).unwrap());
    assert!(g.callees_of("OVL00_CODE::023310").is_empty());
}

#[test]
fn an_overlay_call_into_the_exe_resolves() {
    // Overlays call back into the kernel constantly; the EXE lives in the
    // default space, so it is the fallback when the caller's own space misses.
    let json = r#"{"functions":[
      {"address":"OVL00_CODE::023310","name":"a","block":"OVL00_CODE","markers":[],
       "ops":[{"seq":"OVL00_CODE::023312","op":"CALL","out":null,
               "in":[{"space":"ram","offset":65955,"size":2,"unique":false}]}]},
      {"address":"1000:01a3","name":"kernel","block":"EXE_CODE","markers":[],"ops":[]}
    ],"failures":[]}"#;
    let g = CallGraph::from_pcode(&PcodeExport::from_json(json).unwrap());
    assert_eq!(g.callees_of("OVL00_CODE::023310"), vec!["1000:01a3"]);
}

#[test]
fn functions_outside_a_code_block_are_counted_apart_from_the_gate() {
    // Ghidra creates "functions" in the EXE's data segment, where the overlay
    // data windows live. They are artifacts of decompiling data as code — in
    // the real export 11 of 17 carry a bad marker against 3 of 151 in code — so
    // they must not sit in the population the §8 threshold is measured over.
    // They are reported, not dropped: excluding them raises the headline.
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"a","block":"EXE_CODE","markers":[],"ops":[]},
      {"address":"OVL02_CODE::023770","name":"b","block":"OVL02_CODE","markers":[],"ops":[]},
      {"address":"1e15:245e","name":"c","block":"EXE_DATA","markers":["halt_baddata"],"ops":[]},
      {"address":"1e15:2f9e","name":"d","block":"EXE_DATA","markers":[],"ops":[]}
    ],"failures":[]}"#;
    let e = PcodeExport::from_json(json).unwrap();
    let g = Gate::measure(&e, &CallGraph::from_pcode(&e));
    assert_eq!(
        g.total, 2,
        "only the two code-block functions are the gate population"
    );
    assert_eq!(g.clean, 2);
    assert_eq!(g.clean_ratio, 1.0);
    assert_eq!(
        g.non_code_functions, 2,
        "both EXE_DATA entries are reported"
    );
    assert_eq!(g.non_code_marked, 1);
}

use acad_re::analysis::{record_layout, trace_stores, StoreSite};

#[test]
fn stores_are_returned_in_pcode_order_with_their_offsets() {
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"w","block":"EXE_CODE","markers":[],"ops":[
        {"seq":"1000:0010","op":"STORE","out":null,
         "in":[{"space":"const","offset":0,"size":4,"unique":false},
               {"space":"const","offset":0,"size":2,"unique":false},
               {"space":"register","offset":0,"size":2,"unique":false}]},
        {"seq":"1000:0018","op":"COPY","out":null,"in":[]},
        {"seq":"1000:0020","op":"STORE","out":null,
         "in":[{"space":"const","offset":0,"size":4,"unique":false},
               {"space":"const","offset":2,"size":2,"unique":false},
               {"space":"register","offset":0,"size":8,"unique":false}]}
      ]}],"failures":[]}"#;
    let e = PcodeExport::from_json(json).unwrap();
    let sites = trace_stores(&e, "1000:0003");
    assert_eq!(
        sites,
        vec![
            StoreSite {
                seq: "1000:0010".into(),
                offset: Some(0),
                size: 2
            },
            StoreSite {
                seq: "1000:0020".into(),
                offset: Some(2),
                size: 8
            },
        ]
    );
}

#[test]
fn a_store_through_a_computed_pointer_has_no_constant_offset() {
    // The offset is only meaningful when the pointer operand is a constant.
    // Reporting a register's number as a struct offset would invent a layout.
    let json = r#"{"functions":[
      {"address":"1000:0003","name":"w","block":"EXE_CODE","markers":[],"ops":[
        {"seq":"1000:0010","op":"STORE","out":null,
         "in":[{"space":"const","offset":0,"size":4,"unique":false},
               {"space":"register","offset":8,"size":2,"unique":false},
               {"space":"register","offset":0,"size":2,"unique":false}]}
      ]}],"failures":[]}"#;
    let e = PcodeExport::from_json(json).unwrap();
    assert_eq!(trace_stores(&e, "1000:0003")[0].offset, None);
}

#[test]
fn an_unknown_function_address_traces_to_nothing() {
    let e = PcodeExport::from_json(r#"{"functions":[],"failures":[]}"#).unwrap();
    assert!(trace_stores(&e, "1000:9999").is_empty());
}

#[test]
fn record_layout_sorts_and_deduplicates_offsets() {
    // A writer may store the same field twice; the struct still has one slot.
    let sites = vec![
        StoreSite {
            seq: "a".into(),
            offset: Some(4),
            size: 8,
        },
        StoreSite {
            seq: "b".into(),
            offset: Some(0),
            size: 2,
        },
        StoreSite {
            seq: "c".into(),
            offset: Some(4),
            size: 8,
        },
        StoreSite {
            seq: "d".into(),
            offset: None,
            size: 2,
        },
    ];
    assert_eq!(record_layout(&sites), vec![(0, 2), (4, 8)]);
}
