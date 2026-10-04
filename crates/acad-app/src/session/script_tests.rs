//! Command-script executor contract (docs/native-scripts.md), driven only by
//! a fake clock: no test sleeps, and DELAY is observed as a deadline.
use super::*;
use acad_model::Entity;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
struct FakeClock(AtomicU64);
impl ScriptClock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::SeqCst))
    }
}
impl FakeClock {
    fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

struct Dir(PathBuf);
impl Dir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("acad-script-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn session() -> (Session, Arc<FakeClock>) {
    let mut session = Session::with_editor(acad_cmd::Editor::default(), Libraries::default());
    let clock = Arc::new(FakeClock::default());
    session.set_script_clock(clock.clone());
    (session, clock)
}

fn run(dir: &Dir, name: &str, text: &str) -> (Session, Arc<FakeClock>) {
    let (mut session, clock) = session();
    let path = dir.write(name, text);
    session.start_script(path.to_str().unwrap()).unwrap();
    (session, clock)
}

fn bare(mut entity: &Entity) -> &Entity {
    while let Entity::OnLayer { entity: inner, .. } = entity {
        entity = inner;
    }
    entity
}

fn points(session: &Session) -> Vec<(f64, f64)> {
    session
        .drawing()
        .entities()
        .filter_map(|entity| match bare(entity) {
            Entity::Point { origin } => Some((origin.x, origin.y)),
            _ => None,
        })
        .collect()
}

fn interrupted(session: &Session) -> Option<(ScriptInterrupt, usize)> {
    match session.script_phase()? {
        ScriptPhase::Interrupted { cause, line, .. } => Some((*cause, *line)),
        _ => None,
    }
}

#[test]
fn line_ends_spaces_and_tabs_each_submit_one_return() {
    let dir = Dir::new("syntax");
    let (mut session, _) = run(
        &dir,
        "SYNTAX.SCR",
        "LINE\r\n0,0\r1,0\n2,0 3,0\t\tPOINT 9,9\r\n",
    );
    assert_eq!(
        session.pump_script(),
        ScriptPump {
            quit: false,
            progressed: true
        }
    );
    assert!(session.script_phase().is_none(), "{}", session.status());
    let entities: Vec<_> = session
        .drawing()
        .entities()
        .map(|e| bare(e).clone())
        .collect();
    assert_eq!(
        entities.len(),
        4,
        "three LINE segments then POINT: {entities:?}"
    );
    assert_eq!(points(&session), vec![(9.0, 9.0)]);
    assert_eq!(session.prompt(), "Command");
}

#[test]
fn text_value_prompts_take_the_rest_of_the_line_literally() {
    let dir = Dir::new("text");
    let (mut session, _) = run(&dir, "TEXT.SCR", "TEXT 0,0 1 0 HELLO  WORLD\r\nPOINT 1,1\n");
    session.pump_script();
    let values: Vec<_> = session
        .drawing()
        .entities()
        .filter_map(|entity| match bare(entity) {
            Entity::Text { value, .. } => Some(value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(values, vec!["HELLO  WORLD".to_owned()]);
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
}

#[test]
fn an_unterminated_final_piece_is_left_as_typed_input() {
    let dir = Dir::new("tail");
    let (mut session, _) = run(&dir, "TAIL.SCR", "POINT 1,1\nLINE");
    session.pump_script();
    assert!(session.script_phase().is_none());
    assert_eq!(session.input(), "LINE");
    assert_eq!(session.prompt(), "Command");
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
}

#[test]
fn delay_is_a_clock_deadline_never_a_sleep() {
    let dir = Dir::new("delay");
    let started = std::time::Instant::now();
    let (mut session, clock) = run(&dir, "DELAY.SCR", "POINT 1,1\nDELAY 30000\nPOINT 2,2\n");
    assert_eq!(session.script_wake_in(), Some(Duration::ZERO));
    session.pump_script();
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
    assert!(matches!(
        session.script_phase(),
        Some(ScriptPhase::Delaying { .. })
    ));
    assert_eq!(
        session.script_wake_in(),
        Some(Duration::from_millis(30_000))
    );
    assert_eq!(session.script_status()["delay_remaining_ms"], json!(30_000));

    clock.advance(29_999);
    assert_eq!(session.pump_script(), ScriptPump::default());
    assert_eq!(points(&session).len(), 1);
    assert_eq!(session.script_wake_in(), Some(Duration::from_millis(1)));

    clock.advance(1);
    assert!(session.pump_script().progressed);
    assert_eq!(points(&session), vec![(1.0, 1.0), (2.0, 2.0)]);
    assert!(session.script_phase().is_none());
    assert_eq!(session.script_wake_in(), None);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "a 30 s DELAY must not block the caller"
    );
}

#[test]
fn zero_and_negative_delays_do_not_pause_and_bad_counts_interrupt() {
    let dir = Dir::new("delay-values");
    let (mut session, _) = run(
        &dir,
        "D.SCR",
        "DELAY 0\nDELAY -5\nPOINT 1,1\nDELAY 0.5\nPOINT 2,2\n",
    );
    session.pump_script();
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Error, 4)));
    assert!(
        session.status().contains("DELAY requires an integer"),
        "{}",
        session.status()
    );
}

#[test]
fn an_error_stops_at_the_failing_line_and_resume_continues_after_it() {
    let dir = Dir::new("error");
    let (mut session, _) = run(&dir, "ERR.SCR", "POINT 1,1\nBOGUS\nPOINT 2,2\n");
    session.pump_script();
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Error, 2)));
    assert!(session.status().contains("line 2"), "{}", session.status());
    assert_eq!(session.script_wake_in(), None);
    let status = session.script_status();
    assert_eq!(status["state"], "interrupted");
    assert_eq!(status["next_line"], 3);
    assert_eq!(status["interrupt"]["cause"], "error");
    assert_eq!(status["interrupt"]["line"], 2);

    assert_eq!(
        session.pump_script(),
        ScriptPump::default(),
        "stays stopped"
    );
    assert_eq!(session.command("RESUME"), Ok(false));
    assert_eq!(session.script_phase(), Some(&ScriptPhase::Running));
    session.pump_script();
    assert_eq!(points(&session), vec![(1.0, 1.0), (2.0, 2.0)]);
    assert!(session.script_phase().is_none());
}

#[test]
fn cancel_interrupts_a_delay_and_resume_skips_the_remaining_delay() {
    let dir = Dir::new("cancel");
    let (mut session, _) = run(&dir, "C.SCR", "DELAY 10000\nPOINT 1,1\n");
    session.pump_script();
    assert!(matches!(
        session.script_phase(),
        Some(ScriptPhase::Delaying { .. })
    ));
    session.cancel().unwrap();
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Cancel, 2)));
    assert_eq!(session.script_wake_in(), None);
    session.command("RESUME").unwrap();
    session.pump_script();
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
}

#[test]
fn cancel_after_an_error_interrupt_keeps_the_cursor() {
    let dir = Dir::new("cancel-mid");
    let (mut session, _) = run(&dir, "C.SCR", "LINE 0,0 DELAY\n");
    session.pump_script();
    // "DELAY" was a rejected point; the script stopped at LINE's prompt.
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Error, 1)));
    session.cancel().unwrap();
    assert_eq!(session.prompt(), "Command");
    assert_eq!(
        interrupted(&session),
        Some((ScriptInterrupt::Error, 1)),
        "cancel keeps the cursor"
    );
}

#[test]
fn typed_input_interrupts_and_remains_typed() {
    let dir = Dir::new("typed");
    let (mut session, _) = run(&dir, "T.SCR", "POINT 1,1\nDELAY 100\nPOINT 2,2\n");
    session.pump_script();
    session.type_characters("X").unwrap();
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Input, 3)));
    assert_eq!(session.input(), "X");
    session.set_input(String::new());
    session.command("RESUME").unwrap();
    session.pump_script();
    assert_eq!(points(&session), vec![(1.0, 1.0), (2.0, 2.0)]);
}

#[test]
fn a_session_point_interrupts_a_script_before_its_first_item() {
    let dir = Dir::new("mouse");
    let (mut session, _) = run(&dir, "M.SCR", "POINT\n");
    // Not yet pumped: the point reaches the Command prompt and is rejected,
    // but the script stops before any of its items run.
    assert!(session.point(acad_model::Point { x: 4.0, y: 4.0 }).is_err());
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Input, 1)));
    assert!(
        points(&session).is_empty(),
        "the Command prompt took no point"
    );
}

#[test]
fn long_scripts_run_in_bounded_batches_and_keep_the_host_awake() {
    let dir = Dir::new("long");
    let text: String = (0..200).map(|i| format!("POINT {i},{i}\n")).collect();
    let (mut session, _) = run(&dir, "LONG.SCR", &text);
    let mut pumps = 0;
    while session.script_phase().is_some() {
        let before = points(&session).len();
        session.pump_script();
        pumps += 1;
        assert!(points(&session).len() - before <= SCRIPT_ITEMS_PER_PUMP / 2);
        if session.script_phase().is_some() {
            assert_eq!(session.script_wake_in(), Some(Duration::ZERO));
        }
        assert!(pumps < 100);
    }
    assert_eq!(points(&session).len(), 200);
    assert_eq!(pumps, 400usize.div_ceil(SCRIPT_ITEMS_PER_PUMP));
}

#[test]
fn oversized_missing_and_invalid_scripts_are_refused_before_running() {
    let dir = Dir::new("refuse");
    let (mut session, _) = session();
    let big = dir.write("BIG.SCR", vec![b'\n'; MAX_SCRIPT_BYTES as usize + 1]);
    let error = session.start_script(big.to_str().unwrap()).unwrap_err();
    assert!(error.contains("script file limit"), "{error}");
    let binary = dir.write("BIN.SCR", [0xff, 0xfe, b'\n']);
    assert!(session
        .start_script(binary.to_str().unwrap())
        .unwrap_err()
        .contains("not UTF-8"));
    std::fs::create_dir(dir.0.join("FOLDER.SCR")).unwrap();
    let folder = dir.0.join("FOLDER");
    assert!(session
        .start_script(folder.to_str().unwrap())
        .unwrap_err()
        .contains("script file not found"));
    assert!(session.script_phase().is_none());
    assert_eq!(session.drawing().entities().count(), 0);

    // A failed start keeps an interrupted script resumable.
    let good = dir.write("GOOD.SCR", "BOGUS\nPOINT 1,1\n");
    session.start_script(good.to_str().unwrap()).unwrap();
    session.pump_script();
    assert!(session.start_script("MISSING-SCRIPT").is_err());
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Error, 1)));
}

#[test]
fn script_command_appends_scr_and_searches_the_document_directory() {
    let dir = Dir::new("lookup");
    dir.write("DEMO.SCR", "POINT 3,3\n");
    let (mut session, _) = session();
    session.document.path = Some(dir.0.join("HOST.DWG"));
    session.command("SCRIPT").unwrap();
    assert_eq!(session.prompt(), "SCRIPT: file name");
    session.command("DEMO").unwrap();
    assert_eq!(
        session.script_status()["path"],
        json!(dir.0.join("DEMO.SCR").to_string_lossy())
    );
    session.pump_script();
    assert_eq!(points(&session), vec![(3.0, 3.0)]);
}

#[test]
fn nested_script_is_refused_and_resume_continues_after_it() {
    let dir = Dir::new("nested");
    dir.write("INNER.SCR", "POINT 7,7\n");
    let outer = format!(
        "POINT 1,1\nSCRIPT\n{}\nPOINT 2,2\n",
        dir.0.join("INNER").display()
    );
    let (mut session, _) = run(&dir, "OUTER.SCR", &outer);
    session.pump_script();
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Error, 3)));
    assert!(
        session.status().contains("cannot start another"),
        "{}",
        session.status()
    );
    session.command("RESUME").unwrap();
    session.pump_script();
    assert_eq!(points(&session), vec![(1.0, 1.0), (2.0, 2.0)]);
}

#[test]
fn resume_and_delay_are_inert_without_an_interrupted_script() {
    let dir = Dir::new("inert");
    let (mut session, _) = run(&dir, "R.SCR", "RESUME\nPOINT 1,1\n");
    session.pump_script();
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
    let before = session.drawing().clone();
    assert_eq!(session.command("RESUME"), Ok(false));
    assert!(session.status().contains("no interrupted"));
    session.command("DELAY").unwrap();
    assert_eq!(session.command("100"), Ok(false));
    assert!(session.status().contains("no command script"));
    assert_eq!(session.script_wake_in(), None);
    assert_eq!(session.drawing(), &before);
}

#[test]
fn end_in_a_script_saves_and_exits() {
    let dir = Dir::new("end");
    let (mut session, _) = session();
    let drawing = dir.0.join("A.DWG");
    session.save(&drawing).unwrap();
    let script = dir.write("E.SCR", "POINT 1,1\nEND\nPOINT 2,2\n");
    session.start_script(script.to_str().unwrap()).unwrap();
    assert!(session.pump_script().quit);
    assert!(session.script_phase().is_none());
    let reopened = Session::open(&drawing, &[]).unwrap();
    assert_eq!(points(&reopened), vec![(1.0, 1.0)]);
}

#[test]
fn unnamed_end_takes_the_next_item_as_its_path_and_quit_yes_exits() {
    let dir = Dir::new("end-unnamed");
    let target = dir.0.join("B.DWG");
    let (mut session, _) = run(
        &dir,
        "E.SCR",
        &format!("POINT 1,1\nEND\n{}\n", target.display()),
    );
    assert!(session.pump_script().quit);
    assert!(target.is_file());

    let (mut session, _) = run(&dir, "Q.SCR", "POINT 1,1\nQUIT\nY\n");
    assert!(session.pump_script().quit);
}

#[test]
fn a_failed_save_interrupts_without_exiting() {
    let dir = Dir::new("save-fail");
    let missing = dir.0.join("no-such-directory").join("C.DWG");
    let (mut session, _) = run(
        &dir,
        "F.SCR",
        &format!("END\n{}\nPOINT 1,1\n", missing.display()),
    );
    assert!(!session.pump_script().quit);
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Error, 2)));
    assert!(
        session.status().contains("Save failed"),
        "{}",
        session.status()
    );
    assert!(!missing.exists());
    session.command("RESUME").unwrap();
    session.pump_script();
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
}

#[test]
fn starting_a_script_replaces_an_interrupted_one_and_new_discards_it() {
    let dir = Dir::new("replace");
    let (mut session, _) = run(&dir, "A.SCR", "BOGUS\nPOINT 1,1\n");
    session.pump_script();
    let second = dir.write("B.SCR", "POINT 2,2\n");
    session.start_script(second.to_str().unwrap()).unwrap();
    session.pump_script();
    assert_eq!(points(&session), vec![(2.0, 2.0)]);
    assert_eq!(session.command("RESUME"), Ok(false));
    session.pump_script();
    assert_eq!(points(&session), vec![(2.0, 2.0)], "A was discarded");

    let (mut session, _) = run(&dir, "A.SCR", "BOGUS\nPOINT 1,1\n");
    session.pump_script();
    session.reset();
    assert!(session.script_phase().is_none());
}

#[test]
fn api_starts_reports_ticks_and_stops_scripts() {
    let dir = Dir::new("api");
    let script = dir.write(
        "API.SCR",
        "POINT 1,1\nDELAY 500\nPOINT 2,2\nBOGUS\nPOINT 3,3\n",
    );
    let (mut session, clock) = session();
    let request = |value: serde_json::Value| serde_json::from_value(value).unwrap();
    let result = crate::api::dispatch(
        &mut session,
        request(json!({"method":"script","params":{"path":script.to_str().unwrap()}})),
        (800, 600),
    )
    .unwrap();
    assert_eq!(result["state"]["script"]["state"], "delaying");
    assert_eq!(result["state"]["script"]["delay_remaining_ms"], 500);
    assert_eq!(result["state"]["entities"], 1);

    let status = crate::api::dispatch(
        &mut session,
        request(json!({"method":"script_status","params":{}})),
        (800, 600),
    )
    .unwrap();
    assert_eq!(status["next_line"], 3);
    clock.advance(200);
    let state = crate::api::dispatch(
        &mut session,
        request(json!({"method":"state","params":{}})),
        (800, 600),
    )
    .unwrap();
    assert_eq!(
        state["script"]["delay_remaining_ms"], 300,
        "reads never run items"
    );
    clock.advance(300);
    let result = crate::api::dispatch(
        &mut session,
        request(json!({"method":"script_tick","params":{}})),
        (800, 600),
    )
    .unwrap();
    assert_eq!(result["state"]["entities"], 2);
    assert_eq!(result["state"]["script"]["state"], "interrupted");
    assert_eq!(result["state"]["script"]["interrupt"]["line"], 4);

    let result = crate::api::dispatch(
        &mut session,
        request(json!({"method":"command","params":{"input":"RESUME"}})),
        (800, 600),
    )
    .unwrap();
    assert_eq!(result["state"]["entities"], 3);
    assert_eq!(result["state"]["script"]["state"], "idle");

    crate::api::dispatch(
        &mut session,
        request(json!({"method":"script","params":{"path":script.to_str().unwrap()}})),
        (800, 600),
    )
    .unwrap();
    let result = crate::api::dispatch(
        &mut session,
        request(json!({"method":"script_stop","params":{}})),
        (800, 600),
    )
    .unwrap();
    assert_eq!(result["state"]["script"]["state"], "idle");
    clock.advance(1_000);
    crate::api::dispatch(
        &mut session,
        request(json!({"method":"script_tick","params":{}})),
        (800, 600),
    )
    .unwrap();
    assert_eq!(
        session.drawing().entities().count(),
        4,
        "stopped script ran no more"
    );
}

/// "LINE" then 100 points: one pump (64 items) leaves the script running
/// with LINE's To point prompt open.
fn running_at_an_open_prompt(dir: &Dir) -> (Session, Arc<FakeClock>) {
    let text: String = std::iter::once("LINE\n0,0\n".to_owned())
        .chain((1..100).map(|i| format!("{i},0\n")))
        .collect();
    let (mut session, clock) = run(dir, "OPEN.SCR", &text);
    session.pump_script();
    assert_eq!(session.script_phase(), Some(&ScriptPhase::Running));
    assert_ne!(session.prompt(), "Command");
    (session, clock)
}

#[test]
fn cancel_at_an_open_prompt_interrupts_a_running_script() {
    let dir = Dir::new("cancel-open");
    let (mut session, _) = running_at_an_open_prompt(&dir);
    let lines = session.drawing().entities().count();
    session.cancel().unwrap();
    assert_eq!(session.prompt(), "Command");
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Cancel, 65)));
    assert_eq!(
        session.drawing().entities().count(),
        lines,
        "completed lines kept"
    );
    assert_eq!(session.pump_script(), ScriptPump::default());
}

#[test]
fn clicks_quit_requests_and_input_edits_interrupt_a_running_script() {
    let dir = Dir::new("other-input");
    let (mut session, _) = running_at_an_open_prompt(&dir);
    let _ = session.click(400.0, 300.0, 800, 600);
    assert_eq!(
        interrupted(&session).map(|(cause, _)| cause),
        Some(ScriptInterrupt::Input)
    );

    let (mut session, _) = running_at_an_open_prompt(&dir);
    session.request_quit().unwrap();
    assert_eq!(
        interrupted(&session).map(|(cause, _)| cause),
        Some(ScriptInterrupt::Input)
    );
    assert!(
        session.editor.awaiting_document_input(),
        "QUIT dialogue opened"
    );

    let (mut session, _) = running_at_an_open_prompt(&dir);
    session.set_input("5".into());
    assert_eq!(
        interrupted(&session).map(|(cause, _)| cause),
        Some(ScriptInterrupt::Input)
    );
    assert_eq!(session.input(), "5");
}

fn api(session: &mut Session, value: serde_json::Value) -> serde_json::Value {
    crate::api::dispatch(session, serde_json::from_value(value).unwrap(), (800, 600)).unwrap()
}

#[test]
fn api_point_and_click_interrupt_and_do_not_advance() {
    let dir = Dir::new("api-input");
    for request in [
        json!({"method":"point","params":{"x":50.0,"y":0.0}}),
        json!({"method":"click","params":{"x":400.0,"y":300.0}}),
    ] {
        let (mut session, _) = running_at_an_open_prompt(&dir);
        let before = session.script_status()["next_offset"].clone();
        let result = crate::api::dispatch(
            &mut session,
            serde_json::from_value(request.clone()).unwrap(),
            (800, 600),
        );
        let _ = result;
        let status = session.script_status();
        assert_eq!(status["state"], "interrupted", "{request}");
        assert_eq!(status["interrupt"]["cause"], "input");
        assert_eq!(status["next_offset"], before, "no item ran after {request}");
    }
}

#[test]
fn api_reads_report_and_save_never_advance_a_script() {
    let dir = Dir::new("api-reads");
    let text: String = (0..200).map(|i| format!("POINT {i},{i}\n")).collect();
    let script = dir.write("LONG.SCR", text);
    let (mut session, _) = session();
    let started = api(
        &mut session,
        json!({"method":"script","params":{"path":script.to_str().unwrap()}}),
    );
    assert_eq!(started["state"]["script"]["state"], "running");
    assert_eq!(started["state"]["entities"], 32);
    for request in [
        json!({"method":"state","params":{}}),
        json!({"method":"script_status","params":{}}),
        json!({"method":"drawing","params":{}}),
        json!({"method":"frame","params":{"width":64,"height":64,"format":"rgba"}}),
        json!({"method":"save","params":{"path":dir.0.join("S.DWG").to_str().unwrap()}}),
    ] {
        api(&mut session, request);
        assert_eq!(points(&session).len(), 32);
    }
    session.report = Some(crate::report_view::Report::new(
        (0..200).map(|i| format!("row {i}\n")).collect(),
    ));
    api(
        &mut session,
        json!({"method":"report","params":{"action":"down"}}),
    );
    assert!(session.report_visible(), "the report survives: no item ran");
    assert_eq!(
        points(&session).len(),
        32,
        "report navigation runs no items"
    );
    assert_eq!(
        session.script_phase(),
        Some(&ScriptPhase::Running),
        "and interrupts nothing"
    );
    let mut ticks = 0;
    while session.script_status()["state"] == "running" {
        api(&mut session, json!({"method":"script_tick","params":{}}));
        ticks += 1;
    }
    assert_eq!(points(&session).len(), 200);
    assert_eq!(
        ticks, 6,
        "headless clients tick repeatedly for long scripts"
    );
}

#[test]
fn api_script_interrupts_and_replaces_a_running_script() {
    let dir = Dir::new("api-replace");
    let (mut session, _) = running_at_an_open_prompt(&dir);
    let second = dir.write("SECOND.SCR", "POINT 9,9\n");
    let result = api(
        &mut session,
        json!({"method":"script","params":{"path":second.to_str().unwrap()}}),
    );
    // The open LINE prompt received "POINT" as a point: the replacement's
    // first item fails there, which is what interrupting first means.
    assert_eq!(
        result["state"]["script"]["path"],
        json!(second.to_string_lossy())
    );
    assert_eq!(result["state"]["script"]["state"], "interrupted");
    assert_eq!(result["state"]["script"]["interrupt"]["cause"], "error");
}

#[test]
fn api_open_keeps_the_injected_clock() {
    let dir = Dir::new("api-open");
    let (mut session, clock) = session();
    let drawing = dir.0.join("O.DWG");
    session.save(&drawing).unwrap();
    api(
        &mut session,
        json!({"method":"open","params":{"path":drawing.to_str().unwrap()}}),
    );
    let script = dir.write("D.SCR", "DELAY 500\nPOINT 1,1\n");
    let result = api(
        &mut session,
        json!({"method":"script","params":{"path":script.to_str().unwrap()}}),
    );
    assert_eq!(result["state"]["script"]["delay_remaining_ms"], 500);
    clock.advance(500);
    api(&mut session, json!({"method":"script_tick","params":{}}));
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
}

/// A live group whose first member is already erased: `ERASE 1` makes the
/// ambiguous whole erased owner (docs/native-group-persistence.md, E1).
fn prior_erased_group() -> (Session, Arc<FakeClock>) {
    let mut drawing = acad_dxf::parse(
        b"REPEAT,1\r\nLINE,1\r\n0,0,0,4\r\nLINE,1\r\n1,0,5,0\r\nENDREP,1\r\n1,1,0,0\r\n",
    )
    .unwrap();
    let acad_model::Item::Repeat(r) = &mut drawing.items[0] else {
        panic!("group")
    };
    r.entities[0] = Entity::Erased(Box::new(r.entities[0].clone()));
    let mut session = Session::with_editor(acad_cmd::Editor::new(drawing), Libraries::default());
    let clock = Arc::new(FakeClock::default());
    session.set_script_clock(clock.clone());
    (session, clock)
}

#[test]
fn a_script_item_declining_the_native_erasure_question_interrupts_the_script() {
    let dir = Dir::new("erasure-decline");
    let out = dir.0.join("OUT.DWG");
    let (mut session, _) = prior_erased_group();
    // Written for the original, which never asks: REDRAW would be the answer.
    let script = dir.write(
        "D.SCR",
        format!("ERASE 1\nSAVE\n{}\nREDRAW\nQUIT\nY\n", out.display()),
    );
    session.start_script(script.to_str().unwrap()).unwrap();
    assert!(!session.pump_script().quit);
    assert_eq!(interrupted(&session), Some((ScriptInterrupt::Error, 4)));
    assert!(
        session.status().contains("nothing written") && session.status().contains("line 4"),
        "{}",
        session.status()
    );
    assert_eq!(session.prompt(), "Command");
    assert!(!out.exists());
    assert!(session.is_dirty());
    // RESUME continues after the declining item, as after any script error.
    session.command("RESUME").unwrap();
    assert!(session.pump_script().quit);
    assert!(!out.exists());
}

#[test]
fn an_explicit_y_script_item_confirms_the_native_erasure_question() {
    let dir = Dir::new("erasure-confirm");
    let out = dir.0.join("OUT.DWG");
    let (mut session, _) = prior_erased_group();
    let script = dir.write(
        "Y.SCR",
        format!("ERASE 1\nSAVE\n{}\nY\nPOINT 1,1\n", out.display()),
    );
    session.start_script(script.to_str().unwrap()).unwrap();
    assert!(!session.pump_script().quit);
    assert!(session.script_phase().is_none(), "{}", session.status());
    assert_eq!(points(&session), vec![(1.0, 1.0)]);
    let written = acad_dwg::parse(&std::fs::read(&out).unwrap()).unwrap();
    assert!(
        matches!(&written.items[0], acad_model::Item::Repeat(r) if r.entities.iter().all(Entity::is_erased))
    );
}
