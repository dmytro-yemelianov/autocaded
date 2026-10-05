//! LIST and DBLIST report text: the original's CGA text page against the
//! Rust editor's report for the same typed inputs (QEMU). Blank rows on the
//! original's page are screen positioning, not report content, so the
//! comparison is over the non-blank lines.
#![cfg(unix)]

use acad_cmd::{Editor, Effect};

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() && acad_oracle::available() {
        return Some(disk);
    }
    assert!(
        std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
        "System.img or qemu-system-i386 absent with AUTOCAD_REQUIRE_CORPUS set"
    );
    eprintln!("skipping oracle: extracted System.img or qemu-system-i386 absent");
    None
}

/// The report lines of the original's 80x25 text page, up to its prompt.
fn original_lines(disk: &std::path::Path, name: &str, inputs: &[&str]) -> Vec<String> {
    let probe = acad_oracle::generate_visual_pair(disk, None, name, inputs).unwrap();
    assert_eq!(
        acad_oracle::cga::detect(&probe.cga),
        Some(acad_oracle::cga::Mode::Text),
        "{name}: report page"
    );
    probe.cga[..4000]
        .chunks_exact(160)
        .map(|row| {
            row.chunks_exact(2)
                .map(|cell| char::from(cell[0]))
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .take_while(|line| !line.starts_with("Command:"))
        .filter(|line| !line.is_empty())
        .collect()
}

fn native_lines(inputs: &[&str]) -> Vec<String> {
    let mut editor = Editor::default();
    let mut report = None;
    for input in inputs {
        if let Effect::Report(text) = editor.submit(input).unwrap() {
            report = Some(text);
        }
    }
    report
        .expect("a report")
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn dblist_and_list_match_the_original_report_layout() {
    let Some(disk) = disk() else { return };
    let cases: [(&str, &[&str]); 6] = [
        (
            "ORRDB1",
            &["LINE", "2,3", "8,3", "", "CIRCLE", "5,5", "1.5", "DBLIST"],
        ),
        (
            "ORRDB2",
            &[
                "POINT", "1,2", "ARC", "3,1", "4,2", "5,1", "TEXT", "1,5", "0.5", "30", "Hi there",
                "DBLIST",
            ],
        ),
        (
            "ORRDB3",
            &[
                "SOLID", "0,0", "2,0", "0,2", "2,2", "", "TRACE", "0.25", "3,0", "6,0", "6,3", "",
                "DBLIST",
            ],
        ),
        (
            "ORRDB4",
            &[
                "LINE", "0,0", "1,0", "", "BLOCK", "TK", "0,0", "LAST", "INSERT", "TK", "2,3",
                "1.5", "2", "45", "DBLIST",
            ],
        ),
        (
            "ORRDB5",
            &[
                "REPEAT", "LINE", "1,1", "2,1", "", "POINT", "1,2", "ENDREP", "2", "3", "3", "4",
                "DBLIST",
            ],
        ),
        (
            "ORRLS1",
            &[
                "LINE", "2,3", "8,3", "", "CIRCLE", "5,5", "1.5", "LIST", "L",
            ],
        ),
    ];
    for (name, inputs) in cases {
        assert_eq!(
            native_lines(inputs),
            original_lines(&disk, name, inputs),
            "{name} {inputs:?}"
        );
    }
}
