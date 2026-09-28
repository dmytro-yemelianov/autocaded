use acad_dxf::{parse, write};

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

/// The on-disk file carries cluster slack after its 0x1A terminator;
/// content is everything up to and including that byte.
fn content(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0x1a).unwrap();
    &bytes[..=end]
}

#[test]
fn subdiv_round_trips_byte_identically() {
    let Some(original) = corpus("Samples/SUBDIV.DXF") else {
        return;
    };
    let rewritten = write(&parse(&original).unwrap());
    assert_eq!(rewritten, content(&original));
}

#[test]
fn parse_of_our_own_output_is_stable() {
    let Some(bytes) = corpus("Samples/SUBDIV.DXF") else {
        return;
    };
    let once = parse(&bytes).unwrap();
    let twice = parse(&write(&once)).unwrap();
    assert_eq!(once, twice);
}
