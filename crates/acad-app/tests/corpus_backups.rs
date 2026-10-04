//! Corpus save/reopen workflow with backups (docs/native-files-menu.md).
//! Each of the 21 valid corpus drawings and three other Samples backups is
//! copied into a scratch directory, opened, ENDed and reopened. The corpus
//! itself is only read. An absent corpus is reported, never counted as a
//! pass: set `AUTOCAD_REQUIRE_CORPUS=1` to make its absence a failure.
use acad_app::Session;
use std::path::{Path, PathBuf};

const DRAWINGS: [&str; 21] = [
    "ADDER.DWG",
    "ANDGATE.DWG",
    "BLIVET.DWG",
    "BOX.DWG",
    "COLORS.DWG",
    "DISC.BAK",
    "DLATCH.DWG",
    "FLOOR.DWG",
    "FLOW.DWG",
    "HALFADD.DWG",
    "HOUSE.DWG",
    "INVERTER.DWG",
    "NANDGATE.DWG",
    "NORGATE.DWG",
    "OFFICE.DWG",
    "ORGATE.DWG",
    "SELEXOL.DWG",
    "SHUTTLE.DWG",
    "SUBDIV.DWG",
    "XNORGATE.DWG",
    "XORGATE.DWG",
];
const OTHER_BACKUPS: [&str; 3] = ["COLORS.BAK", "HOUSE.BAK", "OFFICE.BAK"];

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn samples() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/Samples");
    if root.is_dir() {
        return Some(root);
    }
    let message = format!(
        "corpus absent: {} (run ./tools/extract-corpus.sh)",
        root.display()
    );
    if std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_some() {
        panic!("{message}");
    }
    eprintln!("skipping corpus workflow, NOT validated: {message}");
    None
}

/// Units in the last place between two finite values of the same sign.
fn ulps(a: f64, b: f64) -> u64 {
    if a == b {
        return 0;
    }
    if a.is_sign_negative() != b.is_sign_negative() {
        return u64::MAX;
    }
    a.to_bits().abs_diff(b.to_bits())
}

/// Debug renderings equal except numeric tokens within a few ULPs.
fn assert_close(actual: &str, expected: &str, name: &str) {
    let tokens = |text: &str| -> Vec<String> {
        text.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '"')))
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let (actual, expected) = (tokens(actual), tokens(expected));
    assert_eq!(actual.len(), expected.len(), "{name}: structure differs");
    for (a, e) in actual.iter().zip(&expected) {
        match (a.parse::<f64>(), e.parse::<f64>()) {
            (Ok(a), Ok(e)) => assert!(ulps(a, e) <= 4, "{name}: {a} vs {e}"),
            _ => assert_eq!(a, e, "{name}"),
        }
    }
}

#[test]
fn every_corpus_drawing_and_backup_ends_with_a_byte_exact_backup_and_reopens() {
    let Some(root) = samples() else { return };
    let missing: Vec<_> = DRAWINGS
        .iter()
        .chain(&OTHER_BACKUPS)
        .filter(|name| !root.join(name).is_file())
        .collect();
    assert!(missing.is_empty(), "corpus incomplete: missing {missing:?}");
    let scratch =
        Scratch(std::env::temp_dir().join(format!("acad-corpus-backups-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&scratch.0);
    let mut checked = 0;
    for name in DRAWINGS.iter().chain(&OTHER_BACKUPS) {
        let dir = scratch.0.join(name.replace('.', "_"));
        std::fs::create_dir_all(&dir).unwrap();
        let source = std::fs::read(root.join(name)).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, &source).unwrap();
        let mut session = Session::open(&path, &[]).unwrap_or_else(|e| panic!("{name}: {e}"));
        let format = session.document_format().unwrap().to_owned();
        assert!(session.command("END").unwrap(), "{name}");
        if !name.ends_with(".BAK") {
            let backup = std::fs::read(path.with_extension("BAK")).unwrap();
            assert_eq!(backup, source, "{name}: first backup is the original");
        }
        let reopened = Session::open(&path, &[]).unwrap();
        // The pre-existing acad-dwg writers (AC1.2 and AC1.40 alike) may
        // move a value by its last bit on the first rewrite of an original
        // file, so values may differ by a few ULPs; structure, names, flags
        // and order must match exactly.
        assert_close(
            &format!("{:?}", reopened.drawing().items),
            &format!("{:?}", session.drawing().items),
            name,
        );
        assert_eq!(reopened.document_format(), Some(format.as_str()), "{name}");
        let entries = std::fs::read_dir(&dir).unwrap().count();
        if name.ends_with(".BAK") {
            // A .BAK destination is replaced in place without a self-backup.
            assert_eq!(entries, 1, "{name}");
        } else {
            // The drawing and its backup; no staging files remain.
            assert_eq!(entries, 2, "{name}");
        }
        checked += 1;
    }
    assert_eq!(checked, 24);
}
