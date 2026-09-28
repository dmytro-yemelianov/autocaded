//! The committed manifest is the reproducibility record for a corpus whose
//! bytes are deliberately not in git. A record nothing checks is an artifact,
//! not a guarantee, so this recomputes every digest and compares.
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn manifest_rows() -> Vec<BTreeMap<String, String>> {
    let text = fs::read_to_string("../../corpus/manifest.toml")
        .expect("corpus/manifest.toml is committed and must be present");
    text.split("[[file]]").skip(1).map(|block| {
        block.lines().filter_map(|l| l.split_once(" = ")).map(|(k, v)| {
            (k.trim().to_string(), v.trim().trim_matches('"').to_string())
        }).collect()
    }).collect()
}

#[test]
fn every_manifest_row_matches_the_corpus_on_disk() {
    let rows = manifest_rows();
    assert!(!rows.is_empty(), "manifest parsed to zero rows");

    if !Path::new("../../corpus/Samples").is_dir() {
        eprintln!("skipping: corpus not extracted (run ./tools/extract-corpus.sh)");
        return;
    }

    for row in &rows {
        let (disk, name) = (&row["disk"], &row["name"]);
        let path = format!("../../corpus/{disk}/{name}");
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(bytes.len().to_string(), row["bytes"], "{path}: size drifted");
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), row["sha256"],
            "{path}: contents differ from the committed manifest");
    }
}

#[test]
fn the_manifest_names_exactly_one_damaged_file() {
    let damaged: Vec<String> = manifest_rows().iter()
        .filter(|r| r["verdict"].starts_with("corrupt"))
        .map(|r| format!("{}/{} {}", r["disk"], r["name"], r["verdict"]))
        .collect();
    assert_eq!(damaged, vec!["Samples/SHUTTLE.DXF corrupt_at_1536".to_string()],
        "the set of damaged files changed; the spec and tests name SHUTTLE.DXF only");
}
