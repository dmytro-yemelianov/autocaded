//! In-tree original evidence for the Main Menu (docs/native-main-menu.md):
//! task list and prompts, invalid selections, END/QUIT returning to the
//! menu, Configure, File Utilities, and the drawing interchange tasks 5 and
//! 6. Every original behaviour that document cites is asserted here. The
//! System image is only read; drawings are private in-memory copies.
#![cfg(unix)]
use acad_model::{Entity, Point};
use acad_oracle::in_tree::{generate_dwg_in_tree, observe_in_tree};
use std::collections::BTreeMap;

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() {
        return Some(disk);
    }
    assert!(
        std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
        "{} absent with AUTOCAD_REQUIRE_CORPUS set",
        disk.display()
    );
    eprintln!("skipping in-tree Main Menu oracle: extracted System.img absent, NOT validated");
    None
}

struct Run {
    console: String,
    stopped: Option<String>,
    files: BTreeMap<String, Vec<u8>>,
}

const WATCHED: &[&str] = &["D2.DWG", "D2.BAK", "D2.DXF", "D3.DWG", "D3.BAK", "D3.DXF"];

fn run(disk: &std::path::Path, extra: &[(&str, &[u8])], keys: &str) -> Run {
    let observation = observe_in_tree(
        disk,
        b"",
        extra,
        &[(keys.as_bytes(), 400)],
        100_000,
        WATCHED,
    )
    .unwrap();
    Run {
        console: observation.console.replace('\r', ""),
        stopped: observation.stopped,
        files: observation.created.into_iter().collect(),
    }
}

/// Console text after the last Main Menu selection prompt that was answered
/// with `answer`, so assertions see only that task's dialogue.
fn after<'a>(console: &'a str, marker: &str) -> &'a str {
    let at = console
        .rfind(marker)
        .unwrap_or_else(|| panic!("{marker:?} missing\n{console}"));
    &console[at + marker.len()..]
}

fn menu_prompts(console: &str) -> usize {
    console.matches("Enter selection: ").count()
}

const MAIN_MENU: [&str; 8] = [
    "0.  Exit AutoCAD",
    "1.  Begin a NEW drawing",
    "2.  Edit an EXISTING drawing",
    "3.  Plot a drawing",
    "4.  Configure AutoCAD",
    "5.  Make drawing interchange file",
    "6.  Load drawing interchange file",
    "7.  File Utilities",
];

fn entities(drawing: &acad_model::Drawing) -> Vec<Entity> {
    drawing
        .entities()
        .map(|mut entity| {
            while let Entity::OnLayer { entity: inner, .. } = entity {
                entity = inner;
            }
            entity.clone()
        })
        .collect()
}

fn one_line() -> Vec<Entity> {
    vec![Entity::Line {
        start: Point { x: 0.0, y: 0.0 },
        end: Point { x: 5.0, y: 5.0 },
    }]
}

/// D2.DWG holding one LINE (0,0)-(5,5), as the original writes it.
fn source(disk: &std::path::Path) -> Vec<u8> {
    generate_dwg_in_tree(disk, "D2", &["LINE", "0,0", "5,5", ""]).unwrap()
}

/// The original's task 5 output for [`source`].
fn original_dxf(disk: &std::path::Path, dwg: &[u8]) -> Vec<u8> {
    run(disk, &[("D2.DWG", dwg)], "5\rD2\r").files["D2.DXF"].clone()
}

#[test]
fn startup_shows_the_main_menu_and_waits_for_a_selection() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "");
    let menu = after(&run.console, "Main Menu\n");
    let mut position = 0;
    for task in MAIN_MENU {
        let at = menu[position..]
            .find(task)
            .unwrap_or_else(|| panic!("{task}\n{}", run.console));
        position += at + task.len();
    }
    assert!(menu.trim_end().ends_with("Enter selection:"), "{menu}");
    assert_eq!(run.stopped, None);
}

#[test]
fn drawing_tasks_ask_for_the_drawing_name() {
    let Some(disk) = disk() else { return };
    for task in ["1", "2", "3", "5", "6"] {
        let run = run(&disk, &[], &format!("{task}\r"));
        let dialogue = after(&run.console, &format!("Enter selection: {task}"));
        assert_eq!(dialogue.trim(), "Enter NAME of drawing:", "task {task}");
    }
}

#[test]
fn a_blank_or_non_numeric_selection_is_invalid_and_waits_for_return() {
    let Some(disk) = disk() else { return };
    for answer in ["", "X"] {
        let run = run(&disk, &[], &format!("{answer}\r"));
        let dialogue = after(&run.console, &format!("Enter selection: {answer}"));
        assert_eq!(
            dialogue.trim(),
            "** Invalid data entered.\nPress RETURN to return to main menu.",
            "{answer:?}"
        );
        let back = self::run(&disk, &[], &format!("{answer}\r\r"));
        assert_eq!(menu_prompts(&back.console), 2, "{answer:?}");
    }
}

#[test]
fn an_out_of_range_number_redisplays_the_main_menu_without_a_message() {
    let Some(disk) = disk() else { return };
    for answer in ["9", "12"] {
        let run = run(&disk, &[], &format!("{answer}\r"));
        assert_eq!(menu_prompts(&run.console), 2, "{answer}");
        let dialogue = after(&run.console, &format!("Enter selection: {answer}"));
        assert!(!dialogue.contains("Invalid"), "{dialogue}");
        assert!(dialogue.contains("7.  File Utilities"), "{dialogue}");
    }
}

#[test]
fn a_blank_drawing_name_without_default_is_improper() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "2\r\r");
    let dialogue = after(&run.console, "Enter NAME of drawing: ");
    assert_eq!(
        dialogue.trim(),
        "Improper name for drawing.\nPress RETURN to return to main menu."
    );
    assert!(run.files.is_empty());
}

#[test]
fn task_zero_ends_autocad() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "0\r");
    assert_eq!(
        after(&run.console, "Enter selection: 0").trim(),
        "End AutoCAD."
    );
    assert!(run.stopped.is_some(), "the program exits");
}

#[test]
fn end_and_quit_return_to_the_main_menu_naming_the_drawing() {
    let Some(disk) = disk() else { return };
    let dwg = source(&disk);
    for exit in ["END\r", "QUIT\rY\r"] {
        let run = run(&disk, &[("D2.DWG", &dwg)], &format!("2\rD2\r{exit}"));
        assert_eq!(run.stopped, None, "{exit:?}");
        assert_eq!(menu_prompts(&run.console), 2, "{exit:?}");
        let menu = after(&run.console, "Drawing editor.\n");
        assert!(menu.contains("Current drawing:  A:D2\nMain Menu"), "{menu}");
    }
    let new = run(&disk, &[], "1\rD3\rEND\r");
    assert_eq!(menu_prompts(&new.console), 2);
    assert!(new.files.contains_key("D3.DWG"));
}

#[test]
fn declining_the_replace_question_returns_to_the_main_menu() {
    let Some(disk) = disk() else { return };
    let dwg = source(&disk);
    let run = run(&disk, &[("D2.DWG", &dwg)], "1\rD2\rN\r");
    assert_eq!(menu_prompts(&run.console), 2);
    assert!(!run.console.contains("Drawing editor."));
    assert_eq!(run.files["D2.DWG"], dwg);
    assert!(!run.files.contains_key("D2.BAK"));
}

#[test]
fn configure_shows_the_configuration_then_the_configuration_menu() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "4\r\r");
    let shown = after(&run.console, "Enter selection: 4");
    for text in [
        "Configure AutoCAD.",
        "Current AutoCAD configuration",
        "Video display:",
        "Digitizer:",
        "Plotter:",
        "Press RETURN to continue:",
        "Configuration menu",
        "0.  Exit to Main Menu",
        "1.  Show current configuration",
        "2.  Allow I/O port configuration",
        "3.  Configure video display",
        "4.  Configure digitizer",
        "5.  Configure plotter",
        "6.  Configure system console",
        "7.  Configure operating parameters",
    ] {
        assert!(shown.contains(text), "{text}\n{shown}");
    }
    let exit = self::run(&disk, &[], "4\r\r0\r");
    assert!(after(&exit.console, "Enter selection: 0").contains("Keep configuration changes? <Y>"));
}

#[test]
fn file_utilities_open_the_file_utility_menu_and_zero_returns() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "7\r0\r");
    let utilities = after(&run.console, "Enter selection: 7");
    assert!(utilities.contains("File Utility Menu"));
    assert!(utilities.contains("0.  Exit File Utility Menu"));
    assert!(after(&run.console, "Enter selection: 0").contains("Main Menu"));
    assert_eq!(run.stopped, None);
}

#[test]
fn make_dxf_writes_the_interchange_file_beside_the_drawing() {
    let Some(disk) = disk() else { return };
    let dwg = source(&disk);
    let run = run(&disk, &[("D2.DWG", &dwg)], "5\rD2\r");
    assert_eq!(
        after(&run.console, "Enter NAME of drawing: D2").trim(),
        "Make drawing interchange file\nDrawing interchange file complete.\n\
         Press RETURN to return to main menu."
    );
    assert_eq!(run.files["D2.DWG"], dwg, "the drawing is only read");
    // The original writes exactly the native DXF writer's text (CR LF and
    // a ^Z terminator), then NUL padding to a 128-byte record.
    let dxf = &run.files["D2.DXF"];
    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(dxf.len() % 128, 0);
    assert!(dxf[end..].iter().all(|&b| b == 0));
    let native = acad_dxf::write(&acad_dwg::parse(&dwg).unwrap());
    assert_eq!(&dxf[..end], native.as_slice());
    assert_eq!(entities(&acad_dxf::parse(&dxf[..end]).unwrap()), one_line());
}

#[test]
fn make_dxf_replaces_an_existing_file_without_asking_or_backup() {
    let Some(disk) = disk() else { return };
    let dwg = source(&disk);
    let fresh = original_dxf(&disk, &dwg);
    let run = run(&disk, &[("D2.DWG", &dwg), ("D2.DXF", b"OLD")], "5\rD2\r");
    assert!(!run.console.contains("replace"));
    assert_eq!(run.files["D2.DXF"], fresh);
    assert!(!run.files.contains_key("D2.BAK"));
}

#[test]
fn make_dxf_of_a_missing_drawing_reports_and_writes_nothing() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "5\rD3\r");
    assert_eq!(
        after(&run.console, "Enter NAME of drawing: D3").trim(),
        "** No drawing with this name is on file.\nPress RETURN to return to main menu."
    );
    assert!(run.files.is_empty());
}

#[test]
fn load_dxf_makes_a_drawing_with_the_interchange_entities() {
    let Some(disk) = disk() else { return };
    let dwg = source(&disk);
    let dxf = original_dxf(&disk, &dwg);
    let run = run(&disk, &[("D3.DXF", &dxf)], "6\rD3\r");
    assert_eq!(
        after(&run.console, "Enter NAME of drawing: D3").trim(),
        "Reading drawing interchange file\nEnd of drawing interchange file.\n\
         Press RETURN to return to main menu."
    );
    assert!(run.files.contains_key("D3.DWG"));
    assert!(
        !run.console.contains("Drawing editor."),
        "stays at the menu"
    );
    // The original's drawing, edited and ENDed by the original, holds the
    // DXF's entities.
    let ended = self::run(&disk, &[("D3.DXF", &dxf)], "6\rD3\r\r2\rD3\rEND\r");
    let drawing = acad_dwg::parse(&ended.files["D3.DWG"]).unwrap();
    assert_eq!(entities(&drawing), one_line());
}

#[test]
fn load_dxf_into_an_existing_drawing_appends_without_asking_or_backup() {
    let Some(disk) = disk() else { return };
    let dwg = source(&disk);
    let dxf = original_dxf(&disk, &dwg);
    // The existing drawing has other LIMITS and its own LINE.
    let existing = generate_dwg_in_tree(
        &disk,
        "D2",
        &["LIMITS", "1,1", "24,18", "LINE", "1,1", "2,3", ""],
    )
    .unwrap();
    let extra: &[(&str, &[u8])] = &[("D3.DXF", &dxf), ("D3.DWG", &existing)];
    let loaded = run(&disk, extra, "6\rD3\r");
    assert!(!loaded.console.contains("replace"));
    assert!(loaded.console.contains("End of drawing interchange file."));
    assert!(!loaded.files.contains_key("D3.BAK"));
    let ended = run(&disk, extra, "6\rD3\r\r2\rD3\rEND\r");
    let drawing = acad_dwg::parse(&ended.files["D3.DWG"]).unwrap();
    // Existing entities first, then the interchange file's; the
    // interchange file's header values (here LIMITS) replace the drawing's.
    let mut expected = vec![Entity::Line {
        start: Point { x: 1.0, y: 1.0 },
        end: Point { x: 2.0, y: 3.0 },
    }];
    expected.extend(one_line());
    assert_eq!(entities(&drawing), expected);
    let limits = drawing.header.limits;
    assert_eq!(
        (limits.xmin, limits.ymin, limits.xmax, limits.ymax),
        (0.0, 0.0, 12.0, 9.0)
    );
}

#[test]
fn load_dxf_without_the_file_reports_and_keeps_the_drawing() {
    let Some(disk) = disk() else { return };
    let dwg = source(&disk);
    let run = run(&disk, &[("D2.DWG", &dwg)], "6\rD2\r");
    assert_eq!(
        after(&run.console, "Enter NAME of drawing: D2").trim(),
        "Could not open file A:D2.DXF\nPress RETURN to return to main menu."
    );
    assert_eq!(run.files["D2.DWG"], dwg);
}

#[test]
fn load_dxf_format_errors_name_the_line() {
    let Some(disk) = disk() else { return };
    let dwg = source(&disk);
    let existing = run(
        &disk,
        &[("D2.DWG", &dwg), ("D2.DXF", b"LINE,1\r\n1,2\r\n")],
        "6\rD2\r",
    );
    let dialogue = after(&existing.console, "Enter NAME of drawing: D2");
    assert!(
        dialogue.contains("Format error in A:D2.DXF, line 2:\n1,2"),
        "{dialogue}"
    );
    assert!(dialogue.contains("Press RETURN to return to main menu."));
    assert_eq!(existing.files["D2.DWG"], dwg, "existing drawing kept");
    // With no drawing of that name, the original leaves an empty one behind
    // (native policy deliberately writes nothing: docs/native-main-menu.md).
    let new = run(&disk, &[("D3.DXF", b"JUNK,1\r\n1\r\n")], "6\rD3\r");
    assert!(after(&new.console, "Enter NAME of drawing: D3")
        .contains("Format error in A:D3.DXF, line 1:\nJUNK,1"));
    let left = acad_dwg::parse(&new.files["D3.DWG"]).unwrap();
    assert_eq!(left.entities().count(), 0);
}

#[test]
fn plot_is_a_listed_task_that_asks_for_a_drawing_then_plot_settings() {
    let Some(disk) = disk() else { return };
    let dwg = source(&disk);
    let run = run(&disk, &[("D2.DWG", &dwg)], "3\rD2\r");
    let dialogue = after(&run.console, "Enter NAME of drawing: D2");
    assert!(dialogue.contains("Plot drawing."), "{dialogue}");
    assert!(dialogue.contains("Do you want to change anything? <N>"));
}

#[test]
fn a_failed_edit_still_becomes_the_current_drawing() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "2\rD3\r\r");
    let menu = after(&run.console, "Press RETURN to return to main menu.");
    assert!(menu.contains("Current drawing:  A:D3\nMain Menu"), "{menu}");
}

#[test]
fn help_says_end_and_quit_return_to_the_main_menu() {
    let Some(disk) = disk() else { return };
    let run = run(&disk, &[], "1\rD3\rHELP\rEND\rHELP\rQUIT\r");
    let flat = run.console.split_whitespace().collect::<Vec<_>>().join(" ");
    for text in [
        "The END command causes AutoCAD to exit from the Drawing Editor \
         (after saving the updated version of the current drawing), and \
         returns you to the Main Menu.",
        "The QUIT command causes AutoCAD to exit from the Drawing Editor, \
         discarding all updates to the current drawing, and returns you to \
         the Main Menu.",
    ] {
        assert!(flat.contains(text), "{text}\n{}", run.console);
    }
}

/// The File Utility Menu block, from its title to its selection prompt.
fn file_utility_menu(console: &str, after_marker: &str) -> String {
    let tail = after(console, after_marker);
    let start = tail.find("File Utility Menu").expect("File Utility Menu");
    let end = start + tail[start..].find("Enter selection:").expect("prompt");
    tail[start..end].trim_end().to_owned()
}

#[test]
fn task_seven_shows_the_same_menu_as_the_editors_files() {
    let Some(disk) = disk() else { return };
    let task = run(&disk, &[], "7\r");
    let files = run(&disk, &[], "1\rD3\rFILES\r");
    let from_task = file_utility_menu(&task.console, "Enter selection: 7");
    let from_files = file_utility_menu(&files.console, "Drawing editor.");
    assert_eq!(from_task, from_files);
    for item in [
        "0.  Exit File Utility Menu",
        "1.  List Drawing files",
        "2.  List Menu files",
        "3.  List Shape files",
        "4.  List Pattern files",
        "5.  List User specified files",
        "6.  Delete files",
        "7.  Rename files",
    ] {
        assert!(from_task.contains(item), "{item}\n{from_task}");
    }
}

/// An interchange file holding only one LINE (no header records).
const ENTITIES_ONLY: &[u8] = b"LINE,1\r\n0.000000,0.000000,5.000000,5.000000\r\n\x1a";

/// The settings task 6 is checked against.
fn settings(drawing: &acad_model::Drawing) -> String {
    let h = &drawing.header;
    format!(
        "limits {:?} text {:.3} trace {:.3} snap {:?} layers {:?} current {}",
        h.limits, h.text_size, h.trace_width, h.snap, h.layers, h.current_layer
    )
}

#[test]
fn load_dxf_without_header_records_keeps_the_existing_drawing_settings() {
    let Some(disk) = disk() else { return };
    let existing = generate_dwg_in_tree(
        &disk,
        "D2",
        &[
            "LIMITS", "1,1", "24,18", "TEXT", "2,2", "0.5", "0", "HI", "LINE", "1,1", "2,3", "",
        ],
    )
    .unwrap();
    let before = acad_dwg::parse(&existing).unwrap();
    let extra: &[(&str, &[u8])] = &[("D3.DXF", ENTITIES_ONLY), ("D3.DWG", &existing)];
    let ended = run(&disk, extra, "6\rD3\r\r2\rD3\rEND\r");
    let after = acad_dwg::parse(&ended.files["D3.DWG"]).unwrap();
    assert_eq!(settings(&after), settings(&before));
    assert_eq!(after.header.text_size, before.header.text_size);
    let mut expected = entities(&before);
    expected.extend(one_line());
    assert_eq!(entities(&after), expected);
}

#[test]
fn load_dxf_without_header_records_into_a_new_drawing_uses_the_original_defaults() {
    let Some(disk) = disk() else { return };
    let ended = run(&disk, &[("D3.DXF", ENTITIES_ONLY)], "6\rD3\r\r2\rD3\rEND\r");
    let drawing = acad_dwg::parse(&ended.files["D3.DWG"]).unwrap();
    // Recorded so the native difference (docs/native-main-menu.md) is
    // evidenced: these are not the editor's NEW-drawing defaults.
    let limits = drawing.header.limits;
    assert_eq!(
        (limits.xmin, limits.ymin, limits.xmax, limits.ymax),
        (0.0, 0.0, 10.0, 10.0)
    );
    assert!(drawing.header.snap.on);
    assert_eq!(drawing.header.snap.spacing, 0.25);
    assert_eq!(entities(&drawing), one_line());
}
