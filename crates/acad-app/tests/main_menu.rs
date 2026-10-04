//! Native Main Menu (docs/native-main-menu.md) through the shared Session
//! and the typed API. Original wording is asserted against the original in
//! `crates/acad-oracle/tests/main_menu.rs` and `files_backup.rs`.
use acad_app::{api, Session, MAIN_MENU_TASKS};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("acad-main-menu-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn name(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
    fn entries(&self) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn menu() -> Session {
    Session::main_menu(&[])
}
fn ok(s: &mut Session, input: &str) {
    assert_eq!(s.command(input), Ok(false), "{input:?}: {}", s.status());
}
fn fails(s: &mut Session, input: &str) -> String {
    s.command(input).expect_err(input)
}
fn screen(s: &Session) -> Value {
    api::state(s)["main_menu"]["screen"].clone()
}
fn text(s: &Session) -> String {
    s.main_menu_text().expect("Main Menu shown")
}
/// A DWG with one LINE (0,0)-(5,5) written by the native END.
fn line_drawing(path: &Path) -> Vec<u8> {
    let mut s = Session::default();
    for input in ["LINE", "0,0", "5,5", ""] {
        ok(&mut s, input);
    }
    s.save(path).unwrap();
    std::fs::read(path).unwrap()
}
fn entity_count(path: &Path) -> usize {
    Session::open(path, &[])
        .unwrap()
        .drawing()
        .entities()
        .count()
}

#[test]
fn startup_lists_the_eight_tasks_and_asks_for_a_selection() {
    let s = menu();
    assert!(s.main_menu_active() && s.returns_to_main_menu());
    assert_eq!(s.prompt(), "Enter selection");
    assert_eq!(screen(&s), "selection");
    let text = text(&s);
    for (number, task) in MAIN_MENU_TASKS.iter().enumerate() {
        assert!(text.contains(&format!("  {number}.  {task}\n")), "{text}");
    }
    let tasks = &api::state(&s)["main_menu"]["tasks"];
    assert_eq!(tasks.as_array().unwrap().len(), 8);
    assert_eq!(tasks[3]["label"], "Plot a drawing");
}

#[test]
fn invalid_selections_follow_the_original() {
    let mut s = menu();
    for invalid in ["", "X", "-1"] {
        assert_eq!(fails(&mut s, invalid), "** Invalid data entered.");
        assert_eq!(s.prompt(), "Press RETURN to return to main menu");
        ok(&mut s, "");
        assert_eq!(s.prompt(), "Enter selection");
    }
    ok(&mut s, "9");
    assert_eq!(screen(&s), "selection");
    assert!(!text(&s).contains("Invalid"));
}

#[test]
fn task_zero_and_window_close_exit_from_the_main_menu() {
    assert_eq!(menu().command("0"), Ok(true));
    assert_eq!(menu().request_quit(), Ok(true));
}

#[test]
fn new_drawing_then_end_saves_and_returns_to_the_main_menu() {
    let dir = Scratch::new("new");
    let mut s = menu();
    ok(&mut s, "1");
    assert_eq!(s.prompt(), "Enter NAME of drawing");
    ok(&mut s, &dir.name("PLAN"));
    assert!(!s.main_menu_active());
    assert_eq!(s.prompt(), "Command");
    assert_eq!(s.document_path(), Some(dir.0.join("PLAN.DWG").as_path()));
    assert!(dir.entries().is_empty(), "nothing is written before END");
    for input in ["LINE", "0,0", "3,3", ""] {
        ok(&mut s, input);
    }
    assert_eq!(s.command("END"), Ok(false));
    assert!(s.main_menu_active());
    assert_eq!(dir.entries(), ["PLAN.DWG"], "no backup for a new file");
    assert_eq!(entity_count(&dir.0.join("PLAN.DWG")), 1);
    assert!(text(&s).starts_with(&format!(
        "Current drawing:  {}\nMain Menu",
        dir.0.join("PLAN.DWG").display()
    )));
    assert!(s.status().starts_with("Saved "), "{}", s.status());
    // The default name reopens it.
    ok(&mut s, "2");
    assert!(s.prompt().starts_with("Enter NAME of drawing (default "));
    ok(&mut s, "");
    assert_eq!(s.drawing().entities().count(), 1);
}

#[test]
fn new_over_an_existing_name_asks_and_only_end_replaces_with_backup() {
    let dir = Scratch::new("replace");
    let path = dir.0.join("D2.DWG");
    let original = line_drawing(&path);
    let mut s = menu();
    ok(&mut s, "1");
    ok(&mut s, &dir.name("D2"));
    assert_eq!(
        s.prompt(),
        "Do you want to replace it with the new drawing? <N>"
    );
    assert!(text(&s).contains("** Warning!  A drawing with this name already exists."));
    ok(&mut s, "");
    assert_eq!(screen(&s), "selection", "the default N returns to the menu");
    ok(&mut s, "1");
    ok(&mut s, "");
    ok(&mut s, "Y");
    assert_eq!(s.prompt(), "Command");
    assert_eq!(s.drawing().entities().count(), 0);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        original,
        "untouched until END"
    );
    ok(&mut s, "END");
    assert_eq!(std::fs::read(dir.0.join("D2.BAK")).unwrap(), original);
    assert_eq!(entity_count(&path), 0);
}

#[test]
fn edit_existing_and_quit_returns_without_writing() {
    let dir = Scratch::new("edit");
    let path = dir.0.join("D2.DWG");
    let original = line_drawing(&path);
    let mut s = menu();
    ok(&mut s, "2");
    ok(&mut s, &dir.name("D2"));
    assert_eq!(s.drawing().entities().count(), 1);
    for input in ["LINE", "0,0", "9,9", "", "QUIT"] {
        ok(&mut s, input);
    }
    assert_eq!(s.command("Y"), Ok(false));
    assert!(s.main_menu_active());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(dir.entries(), ["D2.DWG"]);
}

#[test]
fn window_close_from_a_main_menu_drawing_exits_the_process() {
    let dir = Scratch::new("close");
    line_drawing(&dir.0.join("D2.DWG"));
    let mut s = menu();
    ok(&mut s, "2");
    ok(&mut s, &dir.name("D2"));
    assert_eq!(s.request_quit(), Ok(false));
    assert_eq!(s.command("Y"), Ok(true));
    // A declined close does not turn a later typed QUIT into an exit.
    let mut s = menu();
    ok(&mut s, "2");
    ok(&mut s, &dir.name("D2"));
    s.request_quit().unwrap();
    ok(&mut s, "N");
    ok(&mut s, "QUIT");
    assert_eq!(s.command("Y"), Ok(false));
    assert!(s.main_menu_active());
}

#[test]
fn missing_and_improper_names_report_and_create_nothing() {
    let dir = Scratch::new("missing");
    let mut s = menu();
    ok(&mut s, "2");
    assert_eq!(fails(&mut s, ""), "Improper name for drawing.");
    ok(&mut s, "");
    for task in ["2", "5"] {
        ok(&mut s, task);
        assert_eq!(
            fails(&mut s, &dir.name("D3")),
            "** No drawing with this name is on file."
        );
        assert_eq!(s.prompt(), "Press RETURN to return to main menu");
        ok(&mut s, "");
    }
    assert!(dir.entries().is_empty());
    assert!(text(&s).starts_with(&format!("Current drawing:  {}", dir.name("D3"))));
}

#[test]
fn make_dxf_writes_the_interchange_file_beside_the_drawing_without_backup() {
    let dir = Scratch::new("make-dxf");
    let path = dir.0.join("D2.DWG");
    let dwg = line_drawing(&path);
    std::fs::write(dir.0.join("D2.DXF"), b"OLD").unwrap();
    let mut s = menu();
    ok(&mut s, "5");
    ok(&mut s, &dir.name("D2"));
    assert!(text(&s).contains("Make drawing interchange file\nDrawing interchange file complete."));
    assert_eq!(s.prompt(), "Press RETURN to return to main menu");
    let expected = acad_dxf::write(&acad_dwg::parse(&dwg).unwrap());
    assert_eq!(std::fs::read(dir.0.join("D2.DXF")).unwrap(), expected);
    assert_eq!(std::fs::read(&path).unwrap(), dwg);
    assert_eq!(dir.entries(), ["D2.DWG", "D2.DXF"]);
    ok(&mut s, "");
    assert_eq!(screen(&s), "selection");
}

#[test]
fn load_dxf_creates_or_appends_to_the_drawing_keeping_a_backup() {
    let dir = Scratch::new("load-dxf");
    let path = dir.0.join("D2.DWG");
    let dwg = line_drawing(&path);
    let dxf = acad_dxf::write(&acad_dwg::parse(&dwg).unwrap());
    std::fs::write(dir.0.join("D3.DXF"), &dxf).unwrap();
    let mut s = menu();
    ok(&mut s, "6");
    ok(&mut s, &dir.name("D3"));
    assert!(text(&s).contains("Reading drawing interchange file\nEnd of drawing interchange file."));
    assert!(s.main_menu_active(), "the original stays at the menu");
    assert_eq!(entity_count(&dir.0.join("D3.DWG")), 1);
    // Into an existing drawing (with other LIMITS): appended, DXF header wins.
    let mut other = Session::default();
    for input in ["LIMITS", "1,1", "24,18", "LINE", "1,1", "2,3", ""] {
        ok(&mut other, input);
    }
    other.save(&dir.0.join("D3.DWG")).unwrap();
    std::fs::remove_file(dir.0.join("D3.BAK")).unwrap();
    let previous = std::fs::read(dir.0.join("D3.DWG")).unwrap();
    ok(&mut s, "");
    ok(&mut s, "6");
    ok(&mut s, &dir.name("D3"));
    let merged = Session::open(&dir.0.join("D3.DWG"), &[]).unwrap();
    let lines: Vec<_> = merged.drawing().entities().collect();
    assert_eq!(lines.len(), 2);
    assert!(
        format!("{:?}", lines[0]).contains("x: 2.0, y: 3.0"),
        "{lines:?}"
    );
    assert_eq!(merged.drawing().header.limits.xmax, 12.0);
    // Native safety extra: the rewritten drawing's previous bytes.
    assert_eq!(std::fs::read(dir.0.join("D3.BAK")).unwrap(), previous);
}

#[test]
fn load_dxf_errors_write_nothing() {
    let dir = Scratch::new("load-errors");
    let mut s = menu();
    ok(&mut s, "6");
    let missing = fails(&mut s, &dir.name("D3"));
    assert_eq!(
        missing,
        format!("Could not open file {}", dir.name("D3.DXF"))
    );
    ok(&mut s, "");
    std::fs::write(dir.0.join("D3.DXF"), b"JUNK,1\r\n1\r\n").unwrap();
    ok(&mut s, "6");
    let error = fails(&mut s, &dir.name("D3"));
    assert!(error.ends_with("D3.DWG not written."), "{error}");
    assert!(text(&s).contains("Format error in "), "{}", text(&s));
    assert_eq!(dir.entries(), ["D3.DXF"], "no empty drawing is left behind");
    // An unreadable existing drawing is never replaced.
    let dwg = line_drawing(&dir.0.join("D2.DWG"));
    let dxf = acad_dxf::write(&acad_dwg::parse(&dwg).unwrap());
    std::fs::write(dir.0.join("D3.DXF"), dxf).unwrap();
    std::fs::write(dir.0.join("D3.DWG"), b"not a drawing").unwrap();
    ok(&mut s, "");
    ok(&mut s, "6");
    assert!(fails(&mut s, &dir.name("D3")).ends_with("not written."));
    assert_eq!(
        std::fs::read(dir.0.join("D3.DWG")).unwrap(),
        b"not a drawing"
    );
}

#[test]
fn configure_shows_the_native_configuration_and_its_menu() {
    let mut s = menu();
    ok(&mut s, "4");
    assert_eq!(s.prompt(), "Press RETURN to continue");
    assert!(text(&s).contains("Current AutoCAD configuration"));
    assert!(text(&s).contains("Plotter:        none"));
    ok(&mut s, "");
    assert_eq!(screen(&s), "configure_menu");
    assert!(text(&s).contains("  0.  Exit to Main Menu\n"));
    assert!(fails(&mut s, "3").contains("not configurable natively"));
    ok(&mut s, "");
    ok(&mut s, "1");
    assert_eq!(screen(&s), "configure_show");
    ok(&mut s, "");
    ok(&mut s, "0");
    assert_eq!(screen(&s), "selection");
}

#[test]
fn plot_is_listed_but_reports_that_it_is_not_available() {
    let mut s = menu();
    assert!(fails(&mut s, "3").contains("plotting is not available"));
    assert_eq!(s.prompt(), "Press RETURN to return to main menu");
}

#[test]
fn file_utilities_reuse_files_and_zero_returns_to_the_main_menu() {
    let dir = Scratch::new("files");
    line_drawing(&dir.0.join("D2.DWG"));
    let mut s = menu();
    ok(&mut s, "7");
    assert_eq!(screen(&s), "file_utilities");
    assert!(s.report_text().unwrap().contains("File Utility Menu"));
    assert_eq!(s.prompt(), "FILES: selection (0–7)");
    ok(&mut s, "5");
    ok(&mut s, &dir.name("*.DWG"));
    assert!(s.report_text().unwrap().contains("D2.DWG"));
    ok(&mut s, "0");
    assert_eq!(screen(&s), "selection");
    assert!(s.report_text().is_none());
    // Escape leaves too.
    ok(&mut s, "7");
    s.cancel().unwrap();
    assert_eq!(screen(&s), "selection");
}

#[test]
fn cancel_returns_to_the_selection_and_drawing_inputs_are_refused() {
    let mut s = menu();
    ok(&mut s, "1");
    s.cancel().unwrap();
    assert_eq!(screen(&s), "selection");
    assert!(s.point(acad_model::Point { x: 1.0, y: 1.0 }).is_err());
    assert!(s.click(10.0, 10.0, 800, 600).is_err());
    assert!(s.save(&std::env::temp_dir().join("never.dwg")).is_err());
    assert!(s.start_script("never").is_err());
    assert_eq!(screen(&s), "selection");
    let frame = s.frame(320, 200).unwrap();
    assert!(frame.pixels.contains(&0x00dd_eeee), "menu text painted");
}

#[test]
fn a_drawing_launch_still_exits_on_end_and_quit() {
    let dir = Scratch::new("launch");
    let path = dir.0.join("D2.DWG");
    line_drawing(&path);
    let mut s = Session::open(&path, &[]).unwrap();
    assert!(!s.returns_to_main_menu() && !s.main_menu_active());
    assert_eq!(s.command("END"), Ok(true));
    let mut s = Session::open(&path, &[]).unwrap();
    ok(&mut s, "QUIT");
    assert_eq!(s.command("Y"), Ok(true));
    assert_eq!(api::state(&s)["main_menu"], Value::Null);
}

fn call(s: &mut Session, method: &str, params: Value) -> Result<Value, String> {
    api::dispatch(
        s,
        serde_json::from_value(json!({"method":method,"params":params})).unwrap(),
        (800, 600),
    )
}

#[test]
fn api_enters_sees_and_answers_the_main_menu() {
    let dir = Scratch::new("api");
    let path = dir.0.join("D2.DWG");
    line_drawing(&path);
    let mut s = Session::default();
    call(&mut s, "command", json!({"input":"POINT"})).unwrap();
    call(&mut s, "command", json!({"input":"1,1"})).unwrap();
    let refused = call(&mut s, "main_menu", json!({})).unwrap_err();
    assert!(refused.contains("unsaved changes"), "{refused}");
    assert_eq!(s.drawing().entities().count(), 1, "nothing discarded");
    call(&mut s, "new", json!({})).unwrap();
    let shown = call(&mut s, "main_menu", json!({})).unwrap();
    assert_eq!(shown["state"]["main_menu"]["screen"], "selection");
    assert_eq!(shown["state"]["prompt"], "Enter selection");
    assert_eq!(shown["state"]["returns_to_main_menu"], true);
    call(&mut s, "command", json!({"input":"2"})).unwrap();
    let opened = call(&mut s, "command", json!({"input":dir.name("D2")})).unwrap();
    assert_eq!(opened["state"]["main_menu"], Value::Null);
    assert_eq!(opened["state"]["entities"], 1);
    let ended = call(&mut s, "command", json!({"input":"END"})).unwrap();
    assert_eq!(ended["quit"], false);
    assert_eq!(ended["state"]["main_menu"]["screen"], "selection");
    // API open keeps the policy; API quit still exits.
    call(&mut s, "open", json!({"path":path})).unwrap();
    assert_eq!(
        call(&mut s, "command", json!({"input":"END"})).unwrap()["quit"],
        false
    );
    assert_eq!(call(&mut s, "quit", json!({})).unwrap()["quit"], true);
}

#[test]
fn a_script_end_returns_to_the_main_menu_and_drops_the_rest_of_the_script() {
    let dir = Scratch::new("script");
    std::fs::write(dir.0.join("RUN.SCR"), "LINE\n0,0\n1,1\n\nEND\n1\n").unwrap();
    let mut s = menu();
    ok(&mut s, "1");
    ok(&mut s, &dir.name("D2"));
    s.start_script(&dir.name("RUN")).unwrap();
    let pump = s.pump_script();
    assert!(!pump.quit);
    assert!(s.main_menu_active());
    assert_eq!(
        screen(&s),
        "selection",
        "the trailing `1` never reaches the menu"
    );
    assert_eq!(api::state(&s)["script"]["state"], "idle");
    assert_eq!(entity_count(&dir.0.join("D2.DWG")), 1);
}

const ENTITIES_ONLY: &[u8] = b"LINE,1\r\n0.000000,0.000000,5.000000,5.000000\r\n\x1a";

#[test]
fn load_dxf_without_header_records_keeps_the_drawing_settings() {
    let dir = Scratch::new("entities-only");
    let mut other = Session::default();
    for input in [
        "LIMITS", "1,1", "24,18", "TEXT", "2,2", "0.5", "0", "HI", "LINE", "1,1", "2,3", "",
    ] {
        ok(&mut other, input);
    }
    other.save(&dir.0.join("D3.DWG")).unwrap();
    let before = other.drawing().header.clone();
    std::fs::write(dir.0.join("D3.DXF"), ENTITIES_ONLY).unwrap();
    let mut s = menu();
    ok(&mut s, "6");
    ok(&mut s, &dir.name("D3"));
    let merged = Session::open(&dir.0.join("D3.DWG"), &[]).unwrap();
    assert_eq!(merged.drawing().header, before, "every setting kept");
    assert_eq!(merged.drawing().entities().count(), 3);
    // A new drawing starts from the native NEW defaults (the original's
    // differ: docs/native-main-menu.md).
    std::fs::write(dir.0.join("D4.DXF"), ENTITIES_ONLY).unwrap();
    ok(&mut s, "");
    ok(&mut s, "6");
    ok(&mut s, &dir.name("D4"));
    let new = Session::open(&dir.0.join("D4.DWG"), &[]).unwrap();
    assert_eq!(
        new.drawing().header,
        Session::default().drawing().header,
        "NEW defaults"
    );
    assert_eq!(new.drawing().entities().count(), 1);
}

#[test]
fn a_cancelled_close_does_not_make_end_or_quit_exit() {
    let dir = Scratch::new("cancelled-close");
    for (exit, answer) in [("END", None), ("QUIT", Some("Y"))] {
        let mut s = menu();
        ok(&mut s, "1");
        ok(&mut s, &dir.name(exit));
        s.request_quit().unwrap();
        s.cancel().unwrap();
        assert_eq!(s.prompt(), "Command");
        match answer {
            None => assert_eq!(s.command(exit), Ok(false)),
            Some(answer) => {
                ok(&mut s, exit);
                assert_eq!(s.command(answer), Ok(false));
            }
        }
        assert!(s.main_menu_active(), "{exit}");
    }
}

#[test]
fn api_main_menu_closes_an_open_file_utilities_dialogue() {
    let mut s = menu();
    ok(&mut s, "7");
    call(&mut s, "main_menu", json!({})).unwrap();
    assert_eq!(screen(&s), "selection");
    ok(&mut s, "7");
    assert_eq!(screen(&s), "file_utilities");
    ok(&mut s, "0");
    assert_eq!(screen(&s), "selection");
}

#[test]
fn an_improper_name_does_not_become_the_current_drawing() {
    let mut s = menu();
    for improper in ["*", "dir/"] {
        ok(&mut s, "2");
        assert_eq!(fails(&mut s, improper), "Improper name for drawing.");
        ok(&mut s, "");
        assert_eq!(api::state(&s)["main_menu"]["current_drawing"], Value::Null);
        assert!(!text(&s).contains("Current drawing"));
    }
}
