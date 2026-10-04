use acad_cmd::Editor;

#[test]
fn grid_supports_zero_relative_spacing_and_on_off_without_altering_snap() {
    let mut e = Editor::default();
    for v in ["SNAP", "0.5", "GRID", "3x"] {
        e.submit(v).unwrap();
    }
    let snap = e.drawing().header.snap;
    assert_eq!(e.drawing().header.grid.spacing, 1.5);
    assert!(e.drawing().header.grid.on);
    for v in ["GRID", "OFF"] {
        e.submit(v).unwrap();
    }
    assert!(!e.drawing().header.grid.on);
    assert_eq!(e.drawing().header.grid.spacing, 1.5);
    for v in ["GRID", "0"] {
        e.submit(v).unwrap();
    }
    assert!(e.drawing().header.grid.on);
    assert_eq!(e.drawing().header.grid.spacing, 0.0);
    for v in ["GRID", "OFF", "GRID", "ON"] {
        e.submit(v).unwrap();
    }
    assert_eq!(e.drawing().header.grid.spacing, 0.0);
    assert_eq!(e.drawing().header.snap, snap);
}

#[test]
fn invalid_grid_spacing_keeps_the_prompt_and_previous_state_for_retry() {
    let mut e = Editor::default();
    for v in ["GRID", "1", "GRID"] {
        e.submit(v).unwrap();
    }
    let before = e.drawing().clone();
    for v in ["-1", "-2X", "NaN", "inf", "X"] {
        assert!(e.submit(v).is_err(), "{v}");
        assert_eq!(e.drawing(), &before);
        assert_eq!(e.prompt(), "GRID: spacing");
    }
    e.submit("2").unwrap();
    assert_eq!(e.drawing().header.grid.spacing, 2.0);
}
