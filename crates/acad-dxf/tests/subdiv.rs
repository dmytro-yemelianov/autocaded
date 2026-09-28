use acad_dxf::parse;
use acad_model::Entity;

/// The corpus is extracted from archives that are deliberately not in git, so
/// a fresh checkout has none. Tests that need it skip rather than fail.
fn corpus(rel: &str) -> Option<Vec<u8>> {
    match std::fs::read(format!("../../corpus/{rel}")) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: corpus/{rel} absent (run ./tools/extract-corpus.sh)");
            None
        }
    }
}

#[test]
fn subdiv_parses_with_the_counts_recorded_in_the_spec() {
    let Some(bytes) = corpus("Samples/SUBDIV.DXF") else {
        return;
    };
    let d = parse(&bytes).unwrap();
    let count = |f: fn(&Entity) -> bool| {
        d.entities()
            .chain(d.blocks().flat_map(|b| b.entities.iter()))
            .filter(|e| f(e))
            .count()
    };
    assert_eq!(count(|e| matches!(e, Entity::Line { .. })), 133);
    assert_eq!(count(|e| matches!(e, Entity::Arc { .. })), 8);
    assert_eq!(count(|e| matches!(e, Entity::Text { .. })), 8);
    assert_eq!(count(|e| matches!(e, Entity::Insert { .. })), 8);
    assert_eq!(count(|e| matches!(e, Entity::Circle { .. })), 2);
    assert_eq!(d.blocks().count(), 6);
    // 78 loose entities + 6 block definitions; the 12 header records are not items.
    assert_eq!(d.items.len(), 84);
}

#[test]
fn shuttle_dxf_is_rejected_as_corrupt_at_1536() {
    let Some(bytes) = corpus("Samples/SHUTTLE.DXF") else {
        return;
    };
    let err = parse(&bytes).unwrap_err();
    assert_eq!(
        format!("{err}"),
        "non-text byte at offset 1536: file is corrupt"
    );
}
