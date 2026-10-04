//! In-tree original evidence for command scripts, DELAY and RESUME
//! (docs/native-scripts.md). AutoCAD 1.4 has no SCRIPT command: `ACAD
//! drawing script` on the DOS command line feeds the script's keys from the
//! Main Menu onwards. Every original behaviour the doc cites is asserted
//! here; the emulator's only clock is its instruction count.
#![cfg(unix)]
use acad_model::{Entity, Point};
use acad_oracle::in_tree::{observe_in_tree, InTreeObservation};

const TAIL: &[u8] = b" D2 S1";
/// Script prefix: Main Menu "Begin a NEW drawing", named D2.
const NEW: &str = "1\r\nD2\r\n";

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() {
        Some(disk)
    } else {
        eprintln!("skipping in-tree oracle: extracted System.img absent");
        None
    }
}

struct Run {
    observation: InTreeObservation,
    /// Bare entities of the saved D2.DWG; `None` when nothing was saved.
    saved: Option<Vec<Entity>>,
    /// Slice (counted across phases) after which D2.DWG first existed.
    saved_at: Option<usize>,
}

fn bare(mut entity: &Entity) -> &Entity {
    while let Entity::OnLayer { entity: inner, .. } = entity {
        entity = inner;
    }
    entity
}

fn run(
    disk: &std::path::Path,
    tail: &[u8],
    script: &[u8],
    phases: &[(&[u8], usize)],
    slice: usize,
) -> Run {
    let observation = observe_in_tree(
        disk,
        tail,
        &[("S1.SCR", script)],
        phases,
        slice,
        &["D2.DWG"],
    )
    .unwrap();
    let saved = observation.created.first().map(|(_, bytes)| {
        acad_dwg::parse(bytes)
            .unwrap()
            .entities()
            .map(|entity| bare(entity).clone())
            .collect()
    });
    let saved_at = observation
        .phases
        .iter()
        .flatten()
        .position(|slice| slice.watched_present > 0);
    Run {
        observation,
        saved,
        saved_at,
    }
}

fn script(disk: &std::path::Path, text: &str) -> Run {
    run(disk, TAIL, text.as_bytes(), &[(b"", 300)], 100_000)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    Entity::Line {
        start: Point { x: x0, y: y0 },
        end: Point { x: x1, y: y1 },
    }
}

#[test]
fn cr_lf_crlf_space_and_tab_each_act_as_one_return() {
    let Some(disk) = disk() else { return };
    let expected = Some(vec![line(0.0, 0.0, 5.0, 5.0)]);
    for text in [
        "1\r\nD2\r\nLINE\r\n0,0\r\n5,5\r\n\r\nEND\r\n",
        "1\rD2\rLINE\r0,0\r5,5\r\rEND\r",
        "1\nD2\nLINE\n0,0\n5,5\n\nEND\n",
        "1\r\nD2\r\nLINE 0,0 5,5  END\r\n",
        "1\r\nD2\r\nLINE\t0,0\t5,5\t\tEND\r\n",
    ] {
        assert_eq!(script(&disk, text).saved, expected, "{text:?}");
    }
}

#[test]
fn the_script_starts_at_the_main_menu_and_an_explicit_extension_is_accepted() {
    let Some(disk) = disk() else { return };
    // A first editor command at the Main Menu is invalid data there.
    let run = script(&disk, "LINE\r\n");
    assert!(run.observation.console.contains("Enter selection: LINE"));
    assert!(run.observation.console.contains("** Invalid data entered."));
    // A blank drawing name takes the command line's current drawing (D2).
    let blank = "1\r\n\r\nLINE\r\n0,0\r\n5,5\r\n\r\nEND\r\n";
    assert_eq!(
        script(&disk, blank).saved,
        Some(vec![line(0.0, 0.0, 5.0, 5.0)])
    );
    let run = run_tail(&disk, b" D2 S1.SCR", &format!("{NEW}END\r\n"));
    assert_eq!(run.saved, Some(vec![]));
}

fn run_tail(disk: &std::path::Path, tail: &[u8], text: &str) -> Run {
    run(disk, tail, text.as_bytes(), &[(b"", 300)], 100_000)
}

#[test]
fn a_missing_script_appends_scr_reports_and_gives_up() {
    let Some(disk) = disk() else { return };
    let run = run_tail(&disk, b" D2 NOPE", "");
    let console = &run.observation.console;
    assert!(
        console.contains("Can't open script file NOPE.SCR"),
        "{console}"
    );
    assert!(console.contains("AutoCAD gives up."));
    assert_eq!(run.observation.stopped.as_deref(), Some("Ok(0)"));
    assert!(run.saved.is_none());
}

#[test]
fn text_values_keep_spaces() {
    let Some(disk) = disk() else { return };
    let run = script(
        &disk,
        &format!("{NEW}TEXT\r\n0,0\r\n1\r\n0\r\nA B\r\nEND\r\n"),
    );
    let saved = run.saved.unwrap();
    assert!(
        matches!(&saved[..], [Entity::Text { value, .. }] if value == "A B"),
        "{saved:?}"
    );
}

#[test]
fn an_unterminated_final_piece_stays_as_typed_input() {
    let Some(disk) = disk() else { return };
    let text = format!("{NEW}END");
    assert!(
        script(&disk, &text).saved.is_none(),
        "END was not submitted"
    );
    let run = run(
        &disk,
        TAIL,
        text.as_bytes(),
        &[(b"", 300), (b"\r", 300)],
        100_000,
    );
    assert_eq!(run.saved, Some(vec![]), "a keyboard Return submits it");
}

#[test]
fn ctrl_z_is_not_an_end_of_file_marker() {
    let Some(disk) = disk() else { return };
    assert!(script(&disk, &format!("{NEW}\x1aEND\r\n")).saved.is_none());
    // 1Ah is an ordinary (unknown) item: it interrupts, and RESUME finds the
    // rest of the file still there. Under an EOF reading nothing would remain.
    let text = format!("{NEW}\x1a\r\nLINE\r\n0,0\r\n5,5\r\n\r\nEND\r\n");
    assert!(script(&disk, &text).saved.is_none());
    let resumed = run(
        &disk,
        TAIL,
        text.as_bytes(),
        &[(b"", 300), (b"RESUME\r", 300)],
        100_000,
    );
    assert_eq!(resumed.saved, Some(vec![line(0.0, 0.0, 5.0, 5.0)]));
}

#[test]
fn an_unknown_command_interrupts_and_resume_continues_after_it() {
    let Some(disk) = disk() else { return };
    let text = format!("{NEW}BOGUS\r\nLINE\r\n0,0\r\n5,5\r\n\r\nEND\r\n");
    assert!(script(&disk, &text).saved.is_none(), "the script stopped");
    let run = run(
        &disk,
        TAIL,
        text.as_bytes(),
        &[(b"", 300), (b"RESUME\r", 300)],
        100_000,
    );
    assert_eq!(run.saved, Some(vec![line(0.0, 0.0, 5.0, 5.0)]));
}

#[test]
fn an_invalid_point_interrupts_and_resume_is_not_a_point() {
    let Some(disk) = disk() else { return };
    let text = format!("{NEW}LINE\r\nfoo\r\n0,0\r\n5,5\r\n\r\nEND\r\n");
    assert!(script(&disk, &text).saved.is_none());
    let run = run(
        &disk,
        TAIL,
        text.as_bytes(),
        &[(b"", 300), (b"RESUME\r", 300)],
        100_000,
    );
    assert!(
        run.saved.is_none(),
        "RESUME at the point prompt resumed nothing"
    );
}

#[test]
fn resume_inside_or_after_a_script_is_harmless() {
    let Some(disk) = disk() else { return };
    let circle = Entity::Circle {
        center: Point { x: 1.0, y: 1.0 },
        radius: 1.0,
    };
    let inside = format!("{NEW}LINE\r\n0,0\r\n5,5\r\n\r\nRESUME\r\nCIRCLE\r\n1,1\r\n1\r\nEND\r\n");
    assert_eq!(
        script(&disk, &inside).saved,
        Some(vec![line(0.0, 0.0, 5.0, 5.0), circle.clone()])
    );
    let finished = format!("{NEW}LINE\r\n0,0\r\n5,5\r\n\r\n");
    let after = run(
        &disk,
        TAIL,
        finished.as_bytes(),
        &[(b"", 300), (b"RESUME\rCIRCLE\r1,1\r1\rEND\r", 300)],
        100_000,
    );
    assert_eq!(after.saved, Some(vec![line(0.0, 0.0, 5.0, 5.0), circle]));
}

/// Slices of 2,000 instructions until D2.DWG exists for `DELAY count`.
fn delay_cost(disk: &std::path::Path, count: &str) -> Option<usize> {
    delay_run(disk, count).saved_at
}

fn delay_run(disk: &std::path::Path, count: &str) -> Run {
    let text = format!("{NEW}DELAY\r\n{count}\r\nEND\r\n");
    run(disk, TAIL, text.as_bytes(), &[(b"", 2_500)], 2_000)
}

/// Cumulative DOS console polls at the slice where D2.DWG first exists.
fn polls_at_save(disk: &std::path::Path, count: &str) -> u64 {
    let run = delay_run(disk, count);
    let at = run.saved_at.unwrap();
    run.observation
        .phases
        .iter()
        .flatten()
        .nth(at)
        .unwrap()
        .console_polls
}

#[test]
fn each_delay_unit_makes_one_console_poll() {
    let Some(disk) = disk() else { return };
    let zero = polls_at_save(&disk, "0");
    let thousand = polls_at_save(&disk, "1000") - zero;
    let two_thousand = polls_at_save(&disk, "2000") - zero;
    assert!(thousand.abs_diff(1000) <= 2, "{thousand}");
    assert!(two_thousand.abs_diff(2000) <= 2, "{two_thousand}");
}

#[test]
fn delay_counts_are_a_cpu_bound_loop_linear_in_the_signed_16_bit_count() {
    let Some(disk) = disk() else { return };
    let zero = delay_cost(&disk, "0").unwrap();
    let thousand = delay_cost(&disk, "1000").unwrap() - zero;
    let two_thousand = delay_cost(&disk, "2000").unwrap() - zero;
    // About 730 emulated 8086 instructions per unit (2,000 per slice).
    assert!((330..=400).contains(&thousand), "{thousand}");
    assert!(
        two_thousand.abs_diff(2 * thousand) <= 3,
        "{two_thousand} vs {thousand}"
    );
    for no_pause in ["-5", "-1000", "32768", "65535"] {
        let cost = delay_cost(&disk, no_pause).unwrap();
        assert!(cost.abs_diff(zero) <= 2, "{no_pause}: {cost} vs {zero}");
    }
    // 70000 wraps to 4464.
    let wrapped = delay_cost(&disk, "70000").unwrap() - zero;
    let expected = thousand * 4464 / 1000;
    assert!(wrapped.abs_diff(expected) <= 25, "{wrapped} vs {expected}");
}

#[test]
fn non_integer_delay_counts_stop_the_script() {
    let Some(disk) = disk() else { return };
    for count in ["0.5", "1e3", "abc", ""] {
        assert!(delay_cost(&disk, count).is_none(), "{count:?}");
        // Stopped, not re-prompting: RESUME continues with END, which saves.
        // A re-prompt would take RESUME as another invalid count.
        let text = format!("{NEW}DELAY\r\n{count}\r\nEND\r\n");
        let resumed = run(
            &disk,
            TAIL,
            text.as_bytes(),
            &[(b"", 500), (b"RESUME\r", 1_000)],
            2_000,
        );
        assert_eq!(resumed.saved, Some(vec![]), "{count:?}");
    }
}

#[test]
fn a_key_during_delay_interrupts_stays_typed_and_resume_skips_the_rest() {
    let Some(disk) = disk() else { return };
    let text = format!("{NEW}DELAY\r\n2000\r\nLINE\r\n0,0\r\n5,5\r\n\r\nEND\r\n");
    let uninterrupted = run(&disk, TAIL, text.as_bytes(), &[(b"", 3_000)], 2_000);
    let full = uninterrupted.saved_at.unwrap();
    // The DELAY runs from about slice 350 to 1,100; press X at slice 600.
    let pressed = run(
        &disk,
        TAIL,
        text.as_bytes(),
        &[(b"", 600), (b"X", 3_000)],
        2_000,
    );
    assert!(pressed.saved.is_none(), "the key interrupted the script");
    let typed = run(
        &disk,
        TAIL,
        text.as_bytes(),
        &[(b"", 600), (b"X", 100), (b"RESUME\r", 3_000)],
        2_000,
    );
    assert!(typed.saved.is_none(), "X stayed typed: XRESUME is unknown");
    let resumed = run(
        &disk,
        TAIL,
        text.as_bytes(),
        &[(b"", 600), (b"X", 100), (b"\x08RESUME\r", 3_000)],
        2_000,
    );
    assert_eq!(resumed.saved, Some(vec![line(0.0, 0.0, 5.0, 5.0)]));
    assert!(
        resumed.saved_at.unwrap() + 200 < full,
        "RESUME did not finish the interrupted delay"
    );
}
