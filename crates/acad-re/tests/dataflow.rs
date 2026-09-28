//! Why §8's third criterion — "the DWG entity record is recoverable as a
//! coherent struct" — is not met, pinned so that an improvement trips it.
//!
//! See docs/re-pipeline.md, "§8 decision gate", for the full write-up.
use acad_re::analysis::trace_stores;
use acad_re::ast::PcodeExport;

/// The full export is gitignored — it is regenerable and tens of megabytes.
fn export() -> Option<PcodeExport> {
    match std::fs::read_to_string("../../build/ast-pcode.json") {
        Ok(s) => Some(PcodeExport::from_json(&s).unwrap()),
        Err(_) => {
            eprintln!("skipping: build/ast-pcode.json absent (run ./tools/re-pipeline.sh)");
            None
        }
    }
}

#[test]
fn no_function_yields_a_constant_offset_store_layout() {
    // `trace_stores` reads a struct layout off `STORE`s whose pointer operand is
    // a constant. In 16-bit segmented code there are none: every store address
    // is built by SEGMENTOP(space, segment, offset), so the pointer is always a
    // computed `unique` varnode. The displacement is not recoverable one or two
    // levels back either — the defining op is a PTRADD whose constant is the
    // element size, not a field offset.
    //
    // That is the whole reason the DWG entity record could not be recovered as
    // a struct. If this test ever fails, constant-offset stores have started
    // appearing and §8's third criterion is worth re-measuring.
    let Some(e) = export() else { return };
    assert!(!e.functions.is_empty(), "the export should not be empty");

    let with_layout: Vec<&str> = e
        .functions
        .iter()
        .filter(|f| {
            trace_stores(&e, &f.address)
                .iter()
                .any(|s| s.offset.is_some())
        })
        .map(|f| f.address.as_str())
        .collect();

    assert!(
        with_layout.is_empty(),
        "constant-offset stores now appear in {} functions ({:?}...) — \
         re-measure the §8 gate's third criterion",
        with_layout.len(),
        &with_layout[..with_layout.len().min(5)]
    );
}

#[test]
fn the_export_still_carries_stores_to_read() {
    // Guards the test above from passing vacuously: if the exporter stopped
    // emitting STOREs at all, "no constant offsets" would be trivially true.
    let Some(e) = export() else { return };
    let stores: usize = e
        .functions
        .iter()
        .map(|f| trace_stores(&e, &f.address).len())
        .sum();
    assert!(
        stores > 1000,
        "expected thousands of STOREs, found {stores}"
    );
}
