use acad_dxf::parse;
use acad_model::Entity;

fn subdiv() -> Vec<u8> {
    std::fs::read("../../corpus/Samples/SUBDIV.DXF")
        .expect("run ./tools/extract-corpus.sh first")
}

#[test]
fn subdiv_parses_with_the_counts_recorded_in_the_spec() {
    let d = parse(&subdiv()).unwrap();
    let count = |f: fn(&Entity) -> bool| d.entities().chain(
        d.blocks().flat_map(|b| b.entities.iter())).filter(|e| f(e)).count();
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
    let bytes = std::fs::read("../../corpus/Samples/SHUTTLE.DXF").unwrap();
    let err = parse(&bytes).unwrap_err();
    assert_eq!(format!("{err}"), "non-text byte at offset 1536: file is corrupt");
}
