use acad_re::{analysis, ovl};

fn corpus(rel: &str) -> Option<Vec<u8>> {
    match std::fs::read(format!("../../corpus/{rel}")) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: corpus/{rel} absent (run ./tools/extract-corpus.sh)");
            None
        }
    }
}

/// ACAD.MNU's screen menu, verbatim (spec §4.3). The full set is a superset.
const MENU: &[&str] = &[
    "LINE", "ARC", "CIRCLE", "TEXT", "INSERT", "MOVE", "COPY", "CHANGE", "FILLET", "ERASE", "OOPS",
    "BREAK", "REDRAW", "POINT", "TRACE", "SOLID", "ARRAY", "HATCH", "SKETCH", "DIM", "ZOOM",
    "LIST", "DIST", "AREA", "STATUS", "TABLET", "FILES", "PLOT", "LIMITS", "GRID",
];

#[test]
fn the_recovered_command_set_covers_the_screen_menu() {
    let Some(bytes) = corpus("System/ACAD.OVL") else {
        return;
    };
    let dir = ovl::parse(&bytes).unwrap();
    let map = analysis::commands(&bytes, &dir);

    let found: std::collections::BTreeSet<&str> =
        map.values().flatten().map(String::as_str).collect();
    let missing: Vec<&str> = MENU
        .iter()
        .copied()
        .filter(|c| !found.contains(c))
        .collect();
    assert!(
        missing.is_empty(),
        "screen-menu commands not recovered: {missing:?}"
    );
}

#[test]
fn the_full_set_is_a_superset_of_the_menu() {
    let Some(bytes) = corpus("System/ACAD.OVL") else {
        return;
    };
    let dir = ovl::parse(&bytes).unwrap();
    let map = analysis::commands(&bytes, &dir);
    let total: usize = map.values().map(Vec::len).sum();
    assert!(
        total > MENU.len(),
        "recovered {total} commands, expected more than {}",
        MENU.len()
    );
}

#[test]
fn the_command_table_is_recovered_whole_and_from_one_overlay() {
    // AutoCAD 1.4 has 57 commands, held in a single space-separated table that
    // the dispatcher indexes. Pinning both numbers catches a filter that has
    // gone loose and started admitting ordinary uppercase message text, and one
    // that has gone tight and started dropping names.
    let Some(bytes) = corpus("System/ACAD.OVL") else {
        return;
    };
    let dir = ovl::parse(&bytes).unwrap();
    let map = analysis::commands(&bytes, &dir);
    assert_eq!(map.len(), 1, "the table lives in exactly one overlay");
    let (entry, names) = map.iter().next().unwrap();
    assert_eq!(*entry, 2, "the table is in entry 2's data region");
    assert_eq!(names.len(), 57, "recovered {names:?}");
    for expected in ["LINE", "QPLOT", "DBLIST", "WBLOCK", "?"] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn every_command_is_attributed_to_an_overlay_that_exists() {
    let Some(bytes) = corpus("System/ACAD.OVL") else {
        return;
    };
    let dir = ovl::parse(&bytes).unwrap();
    for entry in analysis::commands(&bytes, &dir).keys() {
        assert!(
            *entry < dir.entries.len(),
            "overlay {entry} is not in the directory"
        );
    }
}
