//! In-tree original evidence for the Main Menu file tasks, END/QUIT backups
//! and WBLOCK replacement (docs/native-files-menu.md). Every original
//! behaviour that document cites is asserted here. Drawings are supplied to
//! the emulated floppy as private in-memory copies; the System image is only
//! read. Editor prompts are read from the final graphics frame with the
//! original's own display font.
#![cfg(unix)]
use acad_model::{Entity, Point};
use acad_oracle::in_tree::{editor_text_rows, generate_dwg_in_tree, observe_in_tree};
use std::collections::BTreeMap;

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() {
        Some(disk)
    } else {
        assert!(
            std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
            "{} absent",
            disk.display()
        );
        eprintln!("skipping in-tree oracle: extracted System.img absent, NOT validated");
        None
    }
}

struct Run {
    console: String,
    files: BTreeMap<String, Vec<u8>>,
    cga: Vec<u8>,
}

const WATCHED: &[&str] = &["D2.DWG", "D2.BAK", "D3.DWG", "D3.BAK", "D4.DWG"];

fn run(disk: &std::path::Path, extra: &[(&str, &[u8])], keys: &str) -> Run {
    run_with_tail(disk, b"", extra, keys)
}

fn run_with_tail(disk: &std::path::Path, tail: &[u8], extra: &[(&str, &[u8])], keys: &str) -> Run {
    let observation = observe_in_tree(
        disk,
        tail,
        extra,
        &[(keys.as_bytes(), 400)],
        100_000,
        WATCHED,
    )
    .unwrap();
    assert_eq!(observation.stopped, None, "{}", observation.console);
    Run {
        console: observation.console.replace('\r', ""),
        files: observation.created.into_iter().collect(),
        cga: observation.cga,
    }
}

/// The editor's three-line command area (rows 22..=24), as displayed.
fn command_area(disk: &std::path::Path, run: &Run) -> [String; 3] {
    assert_eq!(
        acad_oracle::cga::detect(&run.cga),
        Some(acad_oracle::cga::Mode::Graphics),
        "{}",
        run.console
    );
    let rows = editor_text_rows(disk, &run.cga).unwrap();
    [rows[22].clone(), rows[23].clone(), rows[24].clone()]
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    Entity::Line {
        start: Point { x: x0, y: y0 },
        end: Point { x: x1, y: y1 },
    }
}

fn lines(bytes: &[u8]) -> Vec<Entity> {
    acad_dwg::parse(bytes)
        .unwrap()
        .entities()
        .map(|mut entity| {
            while let Entity::OnLayer { entity: inner, .. } = entity {
                entity = inner;
            }
            entity.clone()
        })
        .collect()
}

/// D2.DWG holding one LINE (0,0)-(5,5), and another drawing to tell apart.
fn sources(disk: &std::path::Path) -> (Vec<u8>, Vec<u8>) {
    let original = generate_dwg_in_tree(disk, "D2", &["LINE", "0,0", "5,5", ""]).unwrap();
    let other = generate_dwg_in_tree(disk, "D2", &["LINE", "1,1", "2,2", ""]).unwrap();
    (original, other)
}

const EDIT_AND_END: &str = "2\rD2\rLINE\r0,0\r9,9\r\rEND\r";

#[test]
fn the_main_menu_offers_eight_tasks_including_file_utilities() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "");
    for task in [
        "0.  Exit AutoCAD",
        "1.  Begin a NEW drawing",
        "2.  Edit an EXISTING drawing",
        "3.  Plot a drawing",
        "4.  Configure AutoCAD",
        "5.  Make drawing interchange file",
        "6.  Load drawing interchange file",
        "7.  File Utilities",
    ] {
        assert!(run.console.contains(task), "{task}\n{}", run.console);
    }
    let files = self::run(&disk, &[], "7\r");
    assert!(files.console.contains("File Utility Menu"));
}

#[test]
fn end_keeps_the_previous_drawing_bytes_as_bak_and_writes_the_edit() {
    let Some(disk) = disk() else { return };
    let (original, _) = sources(&disk);
    let run = run(&disk, &[("D2.DWG", &original)], EDIT_AND_END);
    assert_eq!(run.files["D2.BAK"], original);
    assert_eq!(
        lines(&run.files["D2.DWG"]),
        [line(0.0, 0.0, 5.0, 5.0), line(0.0, 0.0, 9.0, 9.0)]
    );
}

#[test]
fn end_replaces_an_existing_bak() {
    let Some(disk) = disk() else { return };
    let (original, other) = sources(&disk);
    let run = run(
        &disk,
        &[("D2.DWG", &original), ("D2.BAK", &other)],
        EDIT_AND_END,
    );
    assert_eq!(run.files["D2.BAK"], original);
}

#[test]
fn unchanged_end_still_writes_the_drawing_and_its_bak() {
    let Some(disk) = disk() else { return };
    let (original, _) = sources(&disk);
    let run = run(&disk, &[("D2.DWG", &original)], "2\rD2\rEND\r");
    assert_eq!(run.files["D2.BAK"], original);
    assert_eq!(run.files["D2.DWG"], original);
}

#[test]
fn a_second_end_backs_up_the_first_end_output() {
    let Some(disk) = disk() else { return };
    let (original, _) = sources(&disk);
    let run = run(
        &disk,
        &[("D2.DWG", &original)],
        &format!("{EDIT_AND_END}2\rD2\rEND\r"),
    );
    assert!(run
        .console
        .contains("Enter NAME of drawing (default A:D2): D2"));
    let edited = [line(0.0, 0.0, 5.0, 5.0), line(0.0, 0.0, 9.0, 9.0)];
    assert_eq!(lines(&run.files["D2.BAK"]), edited);
    assert_ne!(run.files["D2.BAK"], original);
    assert_eq!(lines(&run.files["D2.DWG"]), edited);
}

#[test]
fn quit_writes_neither_drawing_nor_bak() {
    let Some(disk) = disk() else { return };
    let (original, _) = sources(&disk);
    let run = run(
        &disk,
        &[("D2.DWG", &original)],
        "2\rD2\rLINE\r0,0\r9,9\r\rQUIT\rY\r",
    );
    assert_eq!(run.files["D2.DWG"], original);
    assert!(!run.files.contains_key("D2.BAK"));
}

#[test]
fn a_first_end_of_a_new_drawing_creates_no_bak() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "1\rD2\rEND\r");
    assert!(run.files.contains_key("D2.DWG"));
    assert!(!run.files.contains_key("D2.BAK"));
}

#[test]
fn a_new_drawing_over_an_existing_name_asks_and_end_backs_up_the_old_one() {
    let Some(disk) = disk() else { return };
    let (original, _) = sources(&disk);
    let asked = run(&disk, &[("D2.DWG", &original)], "1\rD2\r");
    assert!(asked
        .console
        .contains("** Warning!  A drawing with this name already exists."));
    assert!(asked
        .console
        .contains("Do you want to replace it with the new drawing? <N>"));
    assert_eq!(asked.files["D2.DWG"], original);
    let replaced = run(&disk, &[("D2.DWG", &original)], "1\rD2\rY\rEND\r");
    assert_eq!(replaced.files["D2.BAK"], original);
    assert_eq!(lines(&replaced.files["D2.DWG"]), []);
}

#[test]
fn editing_a_missing_drawing_reports_and_returns_to_the_main_menu() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "2\rD3\r");
    assert!(run
        .console
        .contains("** No drawing with this name is on file."));
    assert!(run.console.contains("Press RETURN to return to main menu."));
    assert!(run.files.is_empty());
}

#[test]
fn wblock_over_an_existing_file_needs_yes_and_makes_no_bak() {
    let Some(disk) = disk() else { return };
    let (original, other) = sources(&disk);
    let files: &[(&str, &[u8])] = &[("D2.DWG", &original), ("D3.DWG", &other)];
    // Anything but Y (here `*`, which would otherwise name the whole
    // drawing) leaves the destination untouched.
    for answer in ["N", "*"] {
        let kept = run(
            &disk,
            files,
            &format!("2\rD2\rWBLOCK\rD3\r{answer}\rQUIT\rY\r"),
        );
        assert_eq!(kept.files["D3.DWG"], other, "{answer}");
    }
    let replaced = run(&disk, files, "2\rD2\rWBLOCK\rD3\rY\r*\rQUIT\rY\r");
    assert_eq!(lines(&replaced.files["D3.DWG"]), [line(0.0, 0.0, 5.0, 5.0)]);
    assert!(!replaced.files.contains_key("D3.BAK"));
    assert_eq!(replaced.files["D2.DWG"], original);
}

/// Measured prompt text and order: the question follows the file name and
/// precedes the block name; a new name goes straight to the block name.
#[test]
fn wblock_asks_after_the_file_name_and_before_the_block_name() {
    let Some(disk) = disk() else { return };
    let (original, other) = sources(&disk);
    let files: &[(&str, &[u8])] = &[("D2.DWG", &original), ("D3.DWG", &other)];
    let asked = run(&disk, files, "2\rD2\rWBLOCK\rD3\r");
    assert_eq!(
        command_area(&disk, &asked),
        [
            "File name: D3",
            "A drawing with this name already exists.",
            "Do you want to replace it? <N>",
        ]
    );
    assert_eq!(asked.files["D3.DWG"], other);
    let fresh = run(&disk, files, "2\rD2\rWBLOCK\rD4\r");
    assert_eq!(
        command_area(&disk, &fresh),
        ["Command: WBLOCK", "File name: D4", "Block name:"]
    );
    // The original creates the new file, empty, at the file name.
    assert!(fresh.files["D4.DWG"].is_empty());
    // An explicit extension is not a WBLOCK file name.
    let extension = run(&disk, files, "2\rD2\rWBLOCK\rD3.DWG\r");
    assert_eq!(
        command_area(&disk, &extension),
        ["File name: D3.DWG", "*Invalid*", "Command:"]
    );
    assert_eq!(extension.files["D3.DWG"], other);
}

/// Any answer starting with `Y` or `y` continues to the block name; every
/// other answer, including an empty Return, ends WBLOCK and keeps the file.
/// Cancel at the question also keeps it.
#[test]
fn wblock_replace_question_takes_a_leading_y_and_anything_else_is_no() {
    let Some(disk) = disk() else { return };
    let (original, other) = sources(&disk);
    let files: &[(&str, &[u8])] = &[("D2.DWG", &original), ("D3.DWG", &other)];
    for answer in ["y", "Y", "YES", "Yes", "YE", "YEP"] {
        let yes = run(&disk, files, &format!("2\rD2\rWBLOCK\rD3\r{answer}\r"));
        let area = command_area(&disk, &yes);
        assert_eq!(area[1], format!("Do you want to replace it? <N> {answer}"));
        assert_eq!(area[2], "Block name:", "{answer}");
        // `Y` empties the destination at once, before the block name.
        assert!(yes.files["D3.DWG"].is_empty(), "{answer}");
    }
    for answer in ["", "N", "NO", "X", "*"] {
        let no = run(&disk, files, &format!("2\rD2\rWBLOCK\rD3\r{answer}\r"));
        let area = command_area(&disk, &no);
        assert_eq!(
            area[1].trim_end(),
            format!("Do you want to replace it? <N> {answer}").trim_end()
        );
        assert_eq!(area[2], "Command:", "{answer:?}");
        assert_eq!(no.files["D3.DWG"], other, "{answer:?}");
    }
    let empty_then_quit = run(&disk, files, "2\rD2\rWBLOCK\rD3\r\rQUIT\rY\r");
    assert_eq!(empty_then_quit.files["D3.DWG"], other);
    let cancelled = run(&disk, files, "2\rD2\rWBLOCK\rD3\r\x03");
    assert_eq!(
        command_area(&disk, &cancelled)[1..],
        ["Do you want to replace it? <N> *Cancel*", "Command:"]
    );
    assert_eq!(cancelled.files["D3.DWG"], other);
}

/// The open drawing's own file gets the same question and no other guard:
/// after `Y` it holds the WBLOCK output, which QUIT then leaves in place.
#[test]
fn wblock_over_the_open_drawing_asks_the_same_question() {
    let Some(disk) = disk() else { return };
    let (original, other) = sources(&disk);
    let files: &[(&str, &[u8])] = &[("D2.DWG", &original), ("D3.DWG", &other)];
    let asked = run(&disk, files, "2\rD2\rWBLOCK\rD2\r");
    assert_eq!(
        command_area(&disk, &asked),
        [
            "File name: D2",
            "A drawing with this name already exists.",
            "Do you want to replace it? <N>",
        ]
    );
    let edit = "2\rD2\rLINE\r0,0\r9,9\r\r";
    let kept = run(&disk, files, &format!("{edit}WBLOCK\rD2\rN\rQUIT\rY\r"));
    assert_eq!(kept.files["D2.DWG"], original);
    let replaced = run(&disk, files, &format!("{edit}WBLOCK\rD2\rY\r*\rQUIT\rY\r"));
    assert_eq!(
        lines(&replaced.files["D2.DWG"]),
        [line(0.0, 0.0, 5.0, 5.0), line(0.0, 0.0, 9.0, 9.0)]
    );
    assert!(!replaced.files.contains_key("D2.BAK"));
}

/// A command script (`ACAD D2 S1`) answers the question with its next item:
/// `Y` replaces, and an item that is not an answer (`*`) keeps the file.
#[test]
fn a_script_answers_the_wblock_question_with_its_next_item() {
    let Some(disk) = disk() else { return };
    let (original, other) = sources(&disk);
    for (script, replaced) in [
        ("2\r\nD2\r\nWBLOCK\r\nD3\r\nY\r\n*\r\nQUIT\r\nY\r\n", true),
        ("2\r\nD2\r\nWBLOCK\r\nD3\r\n*\r\nQUIT\r\nY\r\n", false),
    ] {
        let files: &[(&str, &[u8])] = &[
            ("D2.DWG", &original),
            ("D3.DWG", &other),
            ("S1.SCR", script.as_bytes()),
        ];
        let run = run_with_tail(&disk, b" D2 S1", files, "");
        if replaced {
            assert_eq!(lines(&run.files["D3.DWG"]), [line(0.0, 0.0, 5.0, 5.0)]);
        } else {
            assert_eq!(run.files["D3.DWG"], other);
        }
        assert_eq!(run.files["D2.DWG"], original);
    }
}
