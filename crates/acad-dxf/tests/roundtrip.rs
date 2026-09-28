use acad_dxf::{parse, write};

fn subdiv() -> Vec<u8> {
    std::fs::read("../../corpus/Samples/SUBDIV.DXF")
        .expect("run ./tools/extract-corpus.sh first")
}

/// The on-disk file carries cluster slack after its 0x1A terminator;
/// content is everything up to and including that byte.
fn content(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0x1a).unwrap();
    &bytes[..=end]
}

#[test]
fn subdiv_round_trips_byte_identically() {
    let original = subdiv();
    let rewritten = write(&parse(&original).unwrap());
    assert_eq!(rewritten, content(&original));
}

#[test]
fn parse_of_our_own_output_is_stable() {
    let once = parse(&subdiv()).unwrap();
    let twice = parse(&write(&once)).unwrap();
    assert_eq!(once, twice);
}
