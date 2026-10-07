use super::*;
use acad_cmd::messages::{Locale, MessageId};

fn unknown(session: &mut Session) {
    assert_eq!(
        session.command("unknown").unwrap_err(),
        "Unknown command. Type ? for list of commands."
    );
    assert_eq!(
        session.status_diagnostic.as_ref().unwrap().message.id(),
        MessageId::ErrorCommandUnknownReturn
    );
}
fn cleared(session: &mut Session) {
    assert!(session.status_diagnostic.is_none());
    assert!(session.submission_diagnostic.is_none());
    let status = session.status().to_owned();
    session.set_locale(Locale::Uk);
    assert_eq!(session.display_status(), status);
}

#[test]
fn diagnostic_lifecycle_captures_raw_macros_and_invalidates_every_status_owner() {
    let mut session = Session::default();
    session.set_locale(Locale::Uk);
    unknown(&mut session);
    assert_eq!(
        session.display_status(),
        "Невідома команда. Введіть ? для списку команд."
    );
    session.rendered_status = "stale render".into();
    session.set_locale(Locale::En);
    assert_eq!(session.display_status(), session.status());
    unknown(&mut session);
    session.set_status(session.status().to_owned());
    cleared(&mut session); // Same prose is still a replacement, not typed identity.
    unknown(&mut session);
    session.command("LINE").unwrap();
    cleared(&mut session);
    session.command("bad point").unwrap_err();
    cleared(&mut session);
    session.cancel().unwrap();
    cleared(&mut session);
    unknown(&mut session);
    session.command("STATUS").unwrap();
    cleared(&mut session);
    session.cancel().unwrap();
    unknown(&mut session);
    session.load_menu("absent-menu-d4");
    cleared(&mut session);
    unknown(&mut session);
    session
        .add_shape_library("INVALID", b"invalid")
        .unwrap_err();
    cleared(&mut session);
    unknown(&mut session);
    session.run_macro("raw{unknown};", &mut |session, result| {
        assert_eq!(
            result.as_ref().unwrap_err(),
            "unknown command: raw{unknown}"
        );
        assert!(session.apply_effect(result).is_err());
    });
    assert_eq!(
        session.status_diagnostic.as_ref().unwrap().message.id(),
        MessageId::ErrorCommandUnknown
    );
    assert_eq!(session.display_status(), "невідома команда: raw{unknown}");
    session.run_macro("line;", &mut |session, result| {
        session.apply_effect(result).unwrap();
    });
    cleared(&mut session);
    session.cancel().unwrap();
    unknown(&mut session);
    let error = crate::api::dispatch(
        &mut session,
        crate::api::Request::Click {
            x: -1.0,
            y: 0.0,
            width: None,
            height: None,
        },
        (800, 600),
    )
    .unwrap_err();
    assert_eq!(session.status(), error);
    cleared(&mut session);
    unknown(&mut session);
    assert!(session.start_script("absent-script-d4").is_err());
    cleared(&mut session);
}

#[test]
fn locale_switch_preserves_pending_input_drawing_view_undo_and_labels() {
    let mut session = Session::default();
    assert_eq!(session.locale(), Locale::En);
    for input in ["LINE", "0,0", "2,3", ""] {
        session.command(input).unwrap();
    }
    session
        .add_shape_library("LOCALE_TEST", b"*129,1,RES\n0;\n")
        .unwrap();
    session.set_palette(acad_model::Palette::Aci256);
    session.command("CIRCLE").unwrap();
    session.set_input("1,2".into());
    let drawing = session.drawing().clone();
    let serialized = acad_dxf::try_write(session.drawing()).unwrap();
    let dwg = acad_dwg::write(session.drawing()).unwrap();
    let palette = session.palette();
    let libraries = session.libraries.clone();
    let mut before = crate::api::state(&session);
    session.set_locale_tag("UK-ua").unwrap();
    assert_eq!(session.locale(), Locale::Uk);
    assert_eq!(session.drawing(), &drawing);
    assert_eq!(acad_dwg::write(session.drawing()).unwrap(), dwg);
    assert_eq!(session.palette(), palette);
    assert!(session.libraries.iter().eq(libraries.iter()));
    assert_eq!(acad_dxf::try_write(session.drawing()).unwrap(), serialized);
    before["prompt"] = serde_json::json!("CIRCLE: центр (або 2P/3P)");
    before["locale"] = serde_json::json!("uk");
    assert_eq!(
        crate::api::state(&session),
        before,
        "only presentation changes"
    );
    assert!(session.set_locale_tag("uk_UA").is_err());
    assert_eq!(session.locale(), Locale::Uk);
    session.cancel().unwrap();
    session.command("UNDO").unwrap();
    assert_eq!(
        session.drawing().entities().count(),
        0,
        "undo survived locale switch"
    );
    assert!(session.command_idle());
    session.reset();
    assert_eq!(session.locale(), Locale::Uk);
    session.open_drawing(drawing);
    assert_eq!(session.locale(), Locale::Uk);
    assert_eq!(
        session.ui_labels()["labels"]["ui.file.open_picker"],
        "Відкрити…"
    );
    assert_eq!(
        session.ui_labels()["labels"]["ui.file.save_picker"],
        "Зберегти як…"
    );
    let menu = Session::main_menu(&[]);
    assert!(!menu.command_idle());
    assert_eq!(menu.main_menu_tasks(), MAIN_MENU_TASKS);
}

#[test]
fn every_session_replacement_inherits_locale_and_standalone_open_defaults_to_english() {
    let root = std::env::temp_dir().join(format!(
        "acad-d4-locale-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("EXISTING.DWG");
    std::fs::write(
        &path,
        acad_dwg::write(acad_cmd::Editor::default().drawing()).unwrap(),
    )
    .unwrap();
    assert_eq!(Session::open(&path, &[]).unwrap().locale(), Locale::En);
    let mut session = Session::default();
    session.set_locale(Locale::Uk);
    crate::api::dispatch(
        &mut session,
        crate::api::Request::Open {
            path: path.clone(),
            directories: vec![],
        },
        (800, 600),
    )
    .unwrap();
    assert_eq!(session.locale(), Locale::Uk);
    session.reset();
    assert_eq!(session.locale(), Locale::Uk);
    session.open_dropped(&path).unwrap();
    assert_eq!(session.locale(), Locale::Uk);
    session.enter_main_menu().unwrap();
    assert_eq!(session.locale(), Locale::Uk);
    assert!(!session.command_idle());
    session.command("1").unwrap();
    session.command(root.join("NEW").to_str().unwrap()).unwrap();
    assert!(!session.main_menu_active(), "NEW adopted a drawing session");
    assert_eq!(session.locale(), Locale::Uk);
    session.enter_main_menu().unwrap();
    session.command("2").unwrap();
    session.command(path.to_str().unwrap()).unwrap();
    assert!(
        !session.main_menu_active(),
        "Edit adopted a drawing session"
    );
    assert_eq!(session.locale(), Locale::Uk);
    let script = root.join("FAIL.SCR");
    std::fs::write(&script, b"unknown\n").unwrap();
    session.start_script(script.to_str().unwrap()).unwrap();
    let running = session.script_status();
    session.set_locale(Locale::En);
    assert_eq!(
        session.script_status(),
        running,
        "pending script survived locale switch"
    );
    session.set_locale(Locale::Uk);
    session.pump_script();
    assert_eq!(session.script_status()["state"], "interrupted");
    assert!(session
        .status()
        .contains("Unknown command. Type ? for list of commands."));
    cleared(&mut session); // Contextual script diagnostics remain canonical English.
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn api_command_failure_keeps_captured_identity_but_other_api_errors_replace_it() {
    let mut session = Session::default();
    crate::api::dispatch(
        &mut session,
        crate::api::Request::SetLocale {
            tag: "uk-UA".into(),
        },
        (800, 600),
    )
    .unwrap();
    let error = crate::api::dispatch(
        &mut session,
        crate::api::Request::Command {
            input: "unknown".into(),
        },
        (800, 600),
    )
    .unwrap_err();
    assert_eq!(error, "Unknown command. Type ? for list of commands.");
    assert!(session.status_diagnostic.is_some());
    session.set_locale(Locale::En);
    assert_eq!(session.display_status(), error);
    let state = crate::api::state(&session);
    assert_eq!(state["command_idle"], true);
    assert_eq!(state["status"], error);
    let error = crate::api::dispatch(
        &mut session,
        crate::api::Request::Save {
            path: "/absent-d4-directory/drawing.DWG".into(),
        },
        (800, 600),
    )
    .unwrap_err();
    assert_eq!(session.status(), error);
    cleared(&mut session);
}

#[test]
fn declared_uk_slice_has_complete_placeholders_tokens_and_bitmap_glyphs() {
    let catalog: serde_json::Value =
        serde_json::from_str(acad_cmd::messages::CATALOG_JSON).unwrap();
    let entries = catalog["messages"].as_array().unwrap();
    assert_eq!(entries.len(), 40);
    for entry in entries {
        let key = entry["key"].as_str().unwrap();
        let uk = entry["text"]["uk"]
            .as_str()
            .expect("complete Ukrainian slice");
        assert!(!uk.is_empty(), "{key}");
        for arg in entry["args"].as_array().unwrap() {
            assert!(uk.contains(&format!("{{{}}}", arg["name"].as_str().unwrap())));
        }
        let en = entry["text"]["en"].as_str().unwrap();
        for token in ["LINE", "CIRCLE", "2P", "3P", "Enter", "D", "?"] {
            if en
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '?')
                .any(|part| part == token)
            {
                assert!(
                    uk.split(|c: char| !c.is_ascii_alphanumeric() && c != '?')
                        .any(|part| part == token),
                    "{key}: {token}"
                );
            }
        }
        if !key.starts_with("ui.file.") {
            for ch in uk.chars().filter(|ch| *ch != '{' && *ch != '}') {
                assert!(crate::bitmap::get_glyph(ch).is_some(), "{key}: {ch:?}");
            }
        }
    }
    let mut menu = Session::main_menu(&[]);
    menu.set_locale(Locale::Uk);
    let tasks = menu.main_menu_tasks();
    assert_eq!(tasks[1], "Почати НОВЕ креслення");
    assert_eq!(tasks[2], "Редагувати НАЯВНЕ креслення");
    assert_eq!(tasks[5], "Створити файл обміну кресленнями");
    for (index, task) in tasks.iter().enumerate() {
        assert!(
            task.chars().count() + 6 <= 40,
            "task {index}: native 640px width"
        );
    }
}

#[test]
fn typed_unknown_rerenders_without_mutation_and_unmigrated_prompts_stay_english() {
    let mut session = Session::default();
    session
        .key_character("not-a-command", KeyModifiers::default())
        .unwrap();
    let input = session.input().to_owned();
    assert!(session.command(&input).is_err());
    assert_eq!(
        session.display_status(),
        "Unknown command. Type ? for list of commands."
    );
    let before = crate::api::state(&session);
    session.set_locale_tag("uk-UA").unwrap();
    assert_eq!(
        session.display_status(),
        "Невідома команда. Введіть ? для списку команд."
    );
    let mut expected = before.clone();
    expected["prompt"] = serde_json::json!("Команда");
    expected["locale"] = serde_json::json!("uk");
    assert_eq!(crate::api::state(&session), expected);
    for malformed in ["", "uk_UA", " uk", "ук"] {
        assert!(session.set_locale_tag(malformed).is_err());
        assert_eq!(session.locale(), Locale::Uk);
        assert_eq!(crate::api::state(&session), expected);
        assert_eq!(
            session.display_status(),
            "Невідома команда. Введіть ? для списку команд."
        );
    }
    session.set_locale_tag("fr-CA").unwrap();
    assert_eq!(session.locale(), Locale::En);
    assert_eq!(crate::api::state(&session), before);
    session.cancel().unwrap();
    assert_eq!(session.display_status(), "");
    session.set_locale(Locale::Uk);
    assert_eq!(session.display_status(), "");
    session.command("ARC").unwrap();
    assert_eq!(session.prompt(), session.editor.prompt());
}

#[test]
fn canonical_commands_produce_identical_drawing_and_file_bytes_in_both_locales() {
    let mut english = Session::default();
    let mut ukrainian = Session::default();
    ukrainian.set_locale(Locale::Uk);
    for input in ["LINE", "0,0", "2,3", "", "CIRCLE", "2P", "0,0", "4,0"] {
        english.command(input).unwrap();
        ukrainian.command(input).unwrap();
    }
    assert_eq!(english.drawing(), ukrainian.drawing());
    assert_eq!(
        acad_dwg::write(english.drawing()).unwrap(),
        acad_dwg::write(ukrainian.drawing()).unwrap()
    );
    assert_eq!(
        acad_dxf::try_write(english.drawing()).unwrap(),
        acad_dxf::try_write(ukrainian.drawing()).unwrap()
    );
}
