use super::*;
use acad_cmd::{messages::Locale, profiles::ProfileId};

#[test]
fn switching_is_presentation_only_and_rejection_is_atomic() {
    let mut s = Session::default();
    assert_eq!(s.profile(), ProfileId::Frozen);
    assert_eq!(s.palette(), acad_model::Palette::Pc16);
    s.command("LINE").unwrap();
    s.command("1,2").unwrap();
    s.command("3,4").unwrap();
    s.set_input("5,6".into());
    s.set_locale(Locale::Uk);
    let drawing = s.drawing().clone();
    let bytes = acad_dxf::try_write(s.drawing()).unwrap();
    let prompt = s.prompt().to_owned();
    let input = s.input().to_owned();
    let status = s.status().to_owned();
    let menu = s.menu.clone();
    let dirty = s.is_dirty();
    let libraries: Vec<_> = s
        .libraries
        .iter()
        .map(|(name, _)| name.to_owned())
        .collect();
    for profile in [ProfileId::Modern, ProfileId::Frozen] {
        s.apply_profile(profile);
        assert_eq!(s.drawing(), &drawing);
        assert_eq!(s.prompt(), prompt);
        assert_eq!(s.input(), input);
        assert_eq!(s.status(), status);
        assert_eq!(s.is_dirty(), dirty);
        assert_eq!(s.locale(), Locale::Uk);
        assert_eq!(s.menu, menu);
        assert_eq!(
            s.libraries
                .iter()
                .map(|(name, _)| name.to_owned())
                .collect::<Vec<_>>(),
            libraries
        );
        assert_eq!(acad_dxf::try_write(s.drawing()).unwrap(), bytes);
    }
    let frame = s.frame(800, 600).unwrap().pixels;
    assert!(s.set_mode("future").is_err());
    assert_eq!(s.frame(800, 600).unwrap().pixels, frame);
    assert_eq!(s.drawing(), &drawing);
    assert_eq!(s.status(), status);
    assert_eq!(s.input(), input);
    assert_eq!(s.profile(), ProfileId::Frozen);
    s.command("5,6").unwrap();
    s.command("").unwrap();
    s.command("UNDO").unwrap();
    assert_eq!(
        s.drawing().entities().count(),
        1,
        "pending command and undo survive switching"
    );
}
#[test]
fn profile_and_manual_palette_survive_replacement_routes_and_locale_changes() {
    let mut s = Session::default();
    s.apply_profile(ProfileId::Modern);
    s.set_palette(acad_model::Palette::Pc16);
    s.set_locale(Locale::Uk);
    for reset in [true, false] {
        if reset {
            s.reset();
        } else {
            s.open_drawing(acad_cmd::Editor::default().drawing().clone());
        }
        assert_eq!(s.profile(), ProfileId::Modern);
        assert_eq!(s.palette(), acad_model::Palette::Pc16);
        assert_eq!(s.locale(), Locale::Uk);
    }
    let mut next = Session::default();
    next.inherit_main_menu_home(&s);
    assert_eq!(next.profile(), ProfileId::Modern);
    assert_eq!(next.palette(), acad_model::Palette::Pc16);
    s = next;
    assert_eq!(s.profile(), ProfileId::Modern);
    assert_eq!(s.palette(), acad_model::Palette::Pc16);
    let uk = s.presentation_profiles();
    s.set_locale(Locale::En);
    let en = s.presentation_profiles();
    assert_ne!(uk["profiles"][0]["title"], en["profiles"][0]["title"]);
    assert_eq!(s.profile(), ProfileId::Modern);
    assert_eq!(s.palette(), acad_model::Palette::Pc16);
    s.apply_profile(ProfileId::Modern);
    assert_eq!(s.palette(), acad_model::Palette::Aci256);
}
#[test]
fn profile_preserves_script_and_macro_progress() {
    let mut s = Session::default();
    s.run_macro("LINE;1,\\3,4;", &mut |session, result| {
        session.apply_effect(result).unwrap();
    });
    let macro_before = s.paused_macro.clone();
    let input_before = s.input().to_owned();
    let path = std::env::temp_dir().join(format!("acad-d8-profile-{}.SCR", std::process::id()));
    std::fs::write(&path, b"LINE\n1,2\n").unwrap();
    s.start_script(path.to_str().unwrap()).unwrap();
    let before = s.script_status();
    s.apply_profile(ProfileId::Modern);
    s.set_locale(Locale::Uk);
    assert_eq!(s.script_status(), before);
    assert_eq!(s.paused_macro, macro_before);
    assert_eq!(s.input(), input_before);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn open_drop_and_main_menu_adopt_retain_profile_with_override() {
    let root = std::env::temp_dir().join(format!("acad-d8-replace-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("EXISTING.DWG");
    std::fs::write(
        &path,
        acad_dwg::write(acad_cmd::Editor::default().drawing()).unwrap(),
    )
    .unwrap();
    let mut s = Session::default();
    s.apply_profile(ProfileId::Modern);
    s.set_palette(acad_model::Palette::Pc16);
    s.set_locale(Locale::Uk);
    fn retained(s: &Session) {
        assert_eq!(s.profile(), ProfileId::Modern);
        assert_eq!(s.palette(), acad_model::Palette::Pc16);
        assert_eq!(s.locale(), Locale::Uk);
    }
    crate::api::dispatch(
        &mut s,
        crate::api::Request::Open {
            path: path.clone(),
            directories: vec![],
        },
        (800, 600),
    )
    .unwrap();
    retained(&s);
    s.reset();
    s.open_dropped(&path).unwrap();
    retained(&s);
    s.enter_main_menu().unwrap();
    s.command("1").unwrap();
    s.command(root.join("NEW").to_str().unwrap()).unwrap();
    assert!(!s.main_menu_active());
    retained(&s);
    s.enter_main_menu().unwrap();
    s.command("2").unwrap();
    s.command(path.to_str().unwrap()).unwrap();
    assert!(!s.main_menu_active());
    retained(&s);
    let state = crate::api::state(&s);
    assert!(crate::api::dispatch(
        &mut s,
        crate::api::Request::SetMode { id: "bad".into() },
        (800, 600)
    )
    .is_err());
    assert_eq!(crate::api::state(&s), state);
    crate::api::dispatch(
        &mut s,
        crate::api::Request::SetMode {
            id: "frozen".into(),
        },
        (800, 600),
    )
    .unwrap();
    assert_eq!(s.profile(), ProfileId::Frozen);
    assert_eq!(s.locale(), Locale::Uk);
    std::fs::remove_dir_all(root).unwrap();
}
