use acad_cmd::{Editor, Effect, FilesFilter, FilesRequest};
use acad_model::{Entity, Item, Point, UnitFormat, Units};

fn bare_first(editor: &Editor) -> &Entity {
    let mut entity = editor.drawing().entities().next().unwrap();
    while let Entity::OnLayer { entity: inner, .. } = entity {
        entity = inner;
    }
    entity
}

#[test]
fn fresh_editor_limits_and_view_match_the_recovered_native_defaults() {
    let mut editor = Editor::default();
    assert_eq!(editor.drawing().entities().count(), 0);
    assert_eq!(
        editor.drawing().header.limits,
        acad_model::Extents {
            xmin: 0.0,
            ymin: 0.0,
            xmax: 12.0,
            ymax: 9.0,
        }
    );
    let view = editor.drawing().header.view;
    assert_eq!(view.height, 9.0);
    assert_eq!(view.center.y, 4.5);
    assert!((view.center.x - 6.850_490_196_078_431).abs() < 1e-12);
    editor.submit("ZOOM").unwrap();
    editor.submit("ALL").unwrap();
    assert_eq!(editor.drawing().header.view, view);
}

#[test]
fn new_editor_preserves_a_supplied_drawing_header() {
    let mut drawing = Editor::default().drawing().clone();
    drawing.header.limits = acad_model::Extents {
        xmin: -5.0,
        ymin: 7.0,
        xmax: 29.0,
        ymax: 35.0,
    };
    drawing.header.view = acad_model::DwgView {
        center: Point { x: 2.5, y: -3.5 },
        height: 11.0,
    };
    let header = drawing.header.clone();
    let editor = Editor::new(drawing);
    assert_eq!(editor.drawing().header, header);
}

#[test]
fn line_keeps_accepting_vertices_until_return_and_undo_removes_the_last_segment() {
    let mut editor = Editor::default();
    editor.submit("LINE").unwrap();
    editor.submit("1.25,2.5").unwrap();
    editor.submit("9.5,6.75").unwrap();
    editor.submit("10,8").unwrap();
    editor.submit("").unwrap();
    assert_eq!(editor.drawing().entities().count(), 2);
    assert_eq!(
        bare_first(&editor),
        &Entity::Line {
            start: Point { x: 1.25, y: 2.5 },
            end: Point { x: 9.5, y: 6.75 }
        }
    );
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing().entities().count(), 1);
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn axis_is_a_display_setting_with_snap_relative_spacing() {
    let mut editor = Editor::default();
    assert_eq!(editor.axis_spacing(), None);
    editor.submit("SNAP").unwrap();
    editor.submit("0.5").unwrap();
    editor.submit("AXIS").unwrap();
    assert_eq!(
        editor.prompt(),
        "AXIS: ON, OFF, or tick spacing (X for snap multiples)"
    );
    editor.submit("5x").unwrap();
    assert_eq!(editor.axis_spacing(), Some(2.5));

    editor.submit("SNAP").unwrap();
    editor.submit("2").unwrap();
    editor.submit("AXIS").unwrap();
    editor.submit("ON").unwrap();
    assert_eq!(
        editor.axis_spacing(),
        Some(2.5),
        "ON retains explicit spacing"
    );
    editor.submit("AXIS").unwrap();
    editor.submit("OFF").unwrap();
    assert_eq!(editor.axis_spacing(), None);
    editor.submit("AXIS").unwrap();
    editor.submit("ON").unwrap();
    assert_eq!(
        editor.axis_spacing(),
        Some(2.5),
        "OFF preserves explicit spacing"
    );

    let mut fresh = Editor::default();
    fresh.submit("SNAP").unwrap();
    fresh.submit("2").unwrap();
    fresh.submit("AXIS").unwrap();
    fresh.submit("ON").unwrap();
    assert_eq!(fresh.drawing().header.axis.spacing, 0.0);
    assert_eq!(
        fresh.axis_spacing(),
        Some(2.0),
        "zero means use SNAP spacing"
    );
}

#[test]
fn axis_spacing_must_be_positive_and_finite() {
    let mut editor = Editor::default();
    editor.submit("AXIS").unwrap();
    for invalid in ["0", "-1", "NaN", "inf"] {
        assert!(editor.submit(invalid).is_err(), "accepted {invalid}");
        assert_eq!(editor.axis_spacing(), None);
    }
}

#[test]
fn mouse_entity_picks_use_selectable_numbers_and_screen_tolerance() {
    let mut editor = Editor::default();
    for input in ["LINE", "0,0", "10,0", "", "CIRCLE", "5,5", "1", "ERASE"] {
        editor.submit(input).unwrap();
    }

    assert!(editor.accepts_mouse_selection());
    assert_eq!(
        editor.pick_entity_at(Point { x: 4.0, y: 0.4 }, 0.5),
        Some(1),
        "the LINE can be picked within the pixel-derived tolerance"
    );
    assert_eq!(
        editor.pick_entity_at(Point { x: 6.0, y: 5.1 }, 0.2),
        Some(2),
        "a CIRCLE is picked by distance to its circumference"
    );
    assert_eq!(editor.pick_entity_at(Point { x: 5.0, y: 5.0 }, 0.2), None);
    assert_eq!(
        editor.pick_entity_at(Point { x: 4.0, y: 0.4 }, f64::NAN),
        None
    );
}

#[test]
fn circle_and_point_use_current_layer_and_report_effects() {
    let mut editor = Editor::default();
    editor.submit("POINT").unwrap();
    editor.submit("2,3").unwrap();
    editor.submit("CIRCLE").unwrap();
    editor.submit("4,5").unwrap();
    editor.submit("2.25").unwrap();
    assert_eq!(editor.drawing().entities().count(), 2);
    assert!(
        matches!(editor.drawing().entities().next(), Some(Entity::OnLayer { layer: 1, entity }) if matches!(entity.as_ref(), Entity::Point { origin: Point { x: 2.0, y: 3.0 } }))
    );
    editor.submit("SAVE").unwrap();
    assert_eq!(
        editor.submit("sample.dwg").unwrap(),
        Effect::Save("sample.dwg".into())
    );
    assert_eq!(editor.submit("END").unwrap(), Effect::End);
}

#[test]
fn load_and_shape_create_the_original_named_shape_records() {
    let mut editor = Editor::default();
    editor.register_shape_library("B:ES.SHP", [("RES".into(), 129), ("CAP".into(), 130)]);
    for input in ["LOAD", "B:ES", "SHAPE"] {
        editor.submit(input).unwrap();
    }
    assert!(editor.submit("MISSING").is_err());
    editor.submit("RES").unwrap();
    assert!(editor.accepts_mouse_point());
    for input in [
        "2.25,3.5", "0.75", "30", "SHAPE", "CAP", "6.5,2.75", "1.25", "75",
    ] {
        editor.submit(input).unwrap();
    }
    assert_eq!(
        editor.drawing().items,
        vec![
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Load {
                    name: "B:ES".into()
                }),
            }),
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Shape {
                    origin: Point { x: 2.25, y: 3.5 },
                    height: 0.75,
                    rotation_deg: 30.0,
                    number: 129,
                }),
            }),
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Shape {
                    origin: Point { x: 6.5, y: 2.75 },
                    height: 1.25,
                    rotation_deg: 75.0,
                    number: 130,
                }),
            }),
        ]
    );
    for _ in 0..3 {
        editor.submit("UNDO").unwrap();
    }
    assert!(editor.drawing().items.is_empty());
    editor.submit("SHAPE").unwrap();
    assert!(editor.submit("RES").is_err());
}

#[test]
fn redraw_and_regen_leave_drawing_data_unchanged() {
    let mut editor = Editor::default();
    for input in ["LINE", "2,3", "8,3", ""] {
        editor.submit(input).unwrap();
    }
    let original = editor.drawing().clone();
    for command in ["REDRAW", "REGEN"] {
        assert_eq!(editor.submit(command).unwrap(), Effect::Continue);
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing(), &original);
    }
}

#[test]
fn cancel_command_returns_to_command_prompt_and_closes_repeat_grouping() {
    let mut editor = Editor::default();
    editor.submit("LINE").unwrap();
    editor.submit("2,3").unwrap();
    assert_eq!(editor.prompt(), "LINE: next point (Enter to finish)");
    editor.cancel_command().unwrap();
    assert_eq!(editor.prompt(), "Command");

    editor.submit("REPEAT").unwrap();
    editor.submit("POINT").unwrap();
    editor.submit("4,5").unwrap();
    assert_eq!(editor.drawing().entities().count(), 1);
    editor.cancel_command().unwrap();
    assert_eq!(editor.prompt(), "Command");
    assert!(editor.submit("REPEAT").is_ok());
}

#[test]
fn insert_rejects_unknown_blocks_without_mutation() {
    let mut editor = Editor::default();
    editor.submit("INSERT").unwrap();
    assert!(editor.submit("MISSING").is_err());
    assert_eq!(editor.prompt(), "INSERT: block or file name");
    assert!(editor.drawing().entities().next().is_none());
}

#[test]
fn star_insert_keeps_load_records_and_moves_shape_geometry() {
    let mut editor = Editor::default();
    editor
        .drawing_mut()
        .items
        .push(Item::Block(acad_model::Block {
            name: "SYMBOL".into(),
            base: Point { x: 1.0, y: 2.0 },
            entities: vec![
                Entity::Load { name: "ES".into() },
                Entity::Shape {
                    origin: Point { x: 2.0, y: 3.0 },
                    height: 1.0,
                    rotation_deg: 0.0,
                    number: 129,
                },
            ],
        }));
    for input in ["INSERT", "*SYMBOL", "7,8"] {
        editor.submit(input).unwrap();
    }
    assert!(matches!(
        &editor.drawing().items[1],
        Item::Entity(Entity::Load { name }) if name == "ES"
    ));
    assert!(matches!(
        &editor.drawing().items[2],
        Item::Entity(Entity::Shape {
            origin: Point { x: 8.0, y: 9.0 },
            number: 129,
            ..
        })
    ));
}

#[test]
fn three_point_arc_uses_middle_point_to_choose_ccw_sweep() {
    let mut editor = Editor::default();
    for input in ["ARC", "4,3", "3,4", "2,3"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(
        bare_first(&editor),
        &Entity::Arc {
            center: Point { x: 3.0, y: 3.0 },
            radius: 1.0,
            start_deg: 0.0,
            end_deg: 180.0
        }
    );
}

#[test]
fn malformed_geometry_does_not_mutate_the_drawing() {
    let mut editor = Editor::default();
    editor.submit("CIRCLE").unwrap();
    editor.submit("1,2").unwrap();
    assert!(editor.submit("-1").is_err());
    assert_eq!(editor.drawing().entities().count(), 0);
}

#[test]
fn zoom_factor_changes_saved_view_height_without_moving_its_center() {
    let mut editor = Editor::default();
    let original = editor.drawing().header.view;
    editor.submit("ZOOM").unwrap();
    editor.submit("2").unwrap();
    assert_eq!(editor.drawing().header.view.height, original.height / 2.0);
    assert_eq!(editor.drawing().header.view.center, original.center);
}

#[test]
fn zoom_previous_restores_the_prior_view() {
    let mut editor = Editor::default();
    let original = editor.drawing().header.view;
    editor.submit("ZOOM").unwrap();
    editor.submit("2").unwrap();
    editor.submit("ZOOM").unwrap();
    editor.submit("P").unwrap();
    assert_eq!(editor.drawing().header.view, original);
}

#[test]
fn zoom_extents_centers_on_the_drawing_and_previous_restores_the_view() {
    let mut editor = Editor::default();
    for input in ["LINE", "-4,2", "6,10", ""] {
        editor.submit(input).unwrap();
    }
    editor.submit("ZOOM").unwrap();
    editor.submit("2").unwrap();
    let zoomed = editor.drawing().header.view;
    editor.submit("ZOOM").unwrap();
    editor.submit("E").unwrap();
    assert_eq!(
        editor.drawing().header.view.center,
        Point { x: 1.0, y: 6.0 }
    );
    assert_eq!(editor.drawing().header.view.height, 8.0);
    editor.submit("ZOOM").unwrap();
    editor.submit("P").unwrap();
    assert_eq!(editor.drawing().header.view, zoomed);
}

#[test]
fn zoom_all_fits_the_union_of_limits_and_extents_anchored_at_the_origin() {
    // Native AutoCAD 1.4's `ZOOM All` (recovered under QEMU in Task 5 of the
    // screen-menu-rendering plan, then *corrected* by the task's own review
    // round after a LIMITS box not containing the origin exposed the first
    // version's bug) does not center a slack-axis box's shown region on the
    // box's own center, and does not anchor at the box's own corner either:
    // it fits `union(LIMITS, EXTENTS)`, and a fresh drawing's `EXTENTS`
    // defaults to a degenerate point *at the origin* (both natively and in
    // this crate's `Editor::default()`), so the shown box always ends up
    // including `(0, 0)` even when `LIMITS` itself does not. `(-10,-10)` to
    // `(10,10)` already contains the origin, so it cannot by itself
    // distinguish "anchored at the box's own corner" from "anchored at the
    // origin" — this test's second case (`LIMITS` not containing the
    // origin at all) is the one that actually pins this down.
    let mut editor = Editor::default();
    for input in ["LIMITS", "-10,-10", "10,10"] {
        editor.submit(input).unwrap();
    }
    editor.submit("ZOOM").unwrap();
    editor.submit("A").unwrap();
    let view = editor.drawing().header.view;
    assert_eq!(view.height, 20.0);
    assert_eq!(
        view.center.y, 0.0,
        "y is the fitting axis: union(LIMITS, origin)'s own center"
    );
    const DEVICE_ASPECT: f64 = 1.522_331_154_684_095_9;
    let expected_x = (20.0 * DEVICE_ASPECT) / 2.0 - 10.0;
    assert!((view.center.x - expected_x).abs() < 1e-9);

    // A LIMITS box that does not contain the origin at all: the shown box
    // must still be anchored relative to (0, 0), not to LIMITS's own
    // (5, 7) lower-left corner — i.e. `union((5,7)-(29,25), (0,0))` =
    // `(0,0)-(29,25)`, not `(5,7)-(29,25)` itself.
    editor.submit("LIMITS").unwrap();
    editor.submit("5,7").unwrap();
    editor.submit("29,25").unwrap();
    editor.submit("ZOOM").unwrap();
    editor.submit("ALL").unwrap();
    let view = editor.drawing().header.view;
    // union box is (0,0)-(29,25): width 29, height 25, aspect 1.16 < device
    // aspect, so height stays exactly 25 (the union box's own height) and
    // width expands from the origin.
    assert_eq!(view.height, 25.0);
    assert_eq!(view.center.y, 12.5, "anchored at union ymin = 0, not 7");
    let expected_x = (25.0 * DEVICE_ASPECT) / 2.0;
    assert!(
        (view.center.x - expected_x).abs() < 1e-6,
        "anchored at union xmin = 0, not LIMITS's own xmin = 5: {} != {expected_x}",
        view.center.x
    );
}

#[test]
fn zoom_window_and_center_set_views_and_reject_empty_extents() {
    let mut editor = Editor::default();
    for input in ["ZOOM", "W", "0,0", "10,5"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(
        editor.drawing().header.view.center,
        Point { x: 5.0, y: 2.5 }
    );
    assert_eq!(
        editor.drawing().header.view.height,
        10.0 / 1.522_331_154_684_095_9
    );
    for input in ["ZOOM", "C", "-2,3", "20"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(
        editor.drawing().header.view.center,
        Point { x: -2.0, y: 3.0 }
    );
    assert_eq!(editor.drawing().header.view.height, 20.0);
    editor.submit("ZOOM").unwrap();
    editor.submit("W").unwrap();
    editor.submit("1,1").unwrap();
    assert!(editor.submit("1,4").is_err());
    assert_eq!(editor.prompt(), "ZOOM WINDOW: upper-right");
}

#[test]
fn pan_changes_view_center_without_changing_height_and_previous_restores_it() {
    let mut editor = Editor::default();
    editor.submit("ZOOM").unwrap();
    editor.submit("2").unwrap();
    let before_pan = editor.drawing().header.view;
    editor.submit("PAN").unwrap();
    editor.submit("@12,-4").unwrap();
    editor.submit("").unwrap();
    assert_eq!(
        editor.drawing().header.view.center,
        Point {
            x: before_pan.center.x - 12.0,
            y: before_pan.center.y + 4.0
        }
    );
    assert_eq!(editor.drawing().header.view.height, before_pan.height);
    editor.submit("ZOOM").unwrap();
    editor.submit("P").unwrap();
    assert_eq!(editor.drawing().header.view, before_pan);
}

#[test]
fn list_erase_and_undo_use_stable_one_based_entity_selection() {
    let mut editor = Editor::default();
    for input in ["POINT", "1,2", "CIRCLE", "4,5", "2"] {
        editor.submit(input).unwrap();
    }
    editor.submit("LIST").unwrap();
    editor.submit("ALL").unwrap();
    assert_eq!(editor.status(), "1 POINT, 2 CIRCLE");
    editor.submit("ERASE").unwrap();
    assert_eq!(editor.prompt(), "ERASE: entity numbers or ALL");
    editor.submit("1").unwrap();
    assert_eq!(editor.drawing().entities().count(), 1);
    assert!(matches!(bare_first(&editor), Entity::Circle { .. }));
    assert_eq!(editor.status(), "Erased 1 entities");
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing().entities().count(), 2);
    assert!(matches!(bare_first(&editor), Entity::Point { .. }));
}

#[test]
fn dblist_reports_live_top_level_block_and_repeat_entities_without_editing() {
    let mut editor = Editor::default();
    editor.drawing_mut().items = vec![
        Item::Entity(Entity::Line {
            start: Point { x: 1.0, y: 2.0 },
            end: Point { x: 3.0, y: 4.0 },
        }),
        Item::Erased(Entity::Point {
            origin: Point { x: 9.0, y: 9.0 },
        }),
        Item::Block(acad_model::Block {
            name: "B1".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![Entity::Circle {
                center: Point { x: 5.0, y: 6.0 },
                radius: 2.0,
            }],
        }),
        Item::Repeat(acad_model::Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![Entity::Point {
                origin: Point { x: 7.0, y: 8.0 },
            }],
            columns: 2,
            rows: 1,
            column_spacing: 10.0,
            row_spacing: 0.0,
        }),
    ];
    let source = editor.drawing().clone();

    let Effect::Report(report) = editor.submit("DBLIST").unwrap() else {
        panic!("DBLIST should return a read-only report");
    };

    assert!(report.contains("Entity 1: Line"));
    assert!(report.contains("Entity 2: Circle"));
    assert!(report.contains("Entity 3: Point"));
    assert!(report.contains("Block B1"));
    assert!(report.contains("Repeat 2 columns × 1 rows"));
    assert!(!report.contains("9.0"), "erased record leaked into DBLIST");
    assert_eq!(editor.drawing(), &source, "DBLIST must be read-only");
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn menu_filename_can_be_cancelled_and_files_displays_the_utility_menu() {
    let mut editor = Editor::default();
    editor.submit("MENU").unwrap();
    assert_eq!(editor.prompt(), "File name");
    editor.submit("").unwrap();
    assert_eq!(editor.prompt(), "Command");

    let Effect::Report(menu) = editor.submit("FILES").unwrap() else {
        panic!("FILES should display its utility menu");
    };
    assert!(menu.contains("File Utility Menu"));
    assert!(menu.contains("List Drawing files"));
    assert!(menu.contains("Rename files"));
    assert_eq!(editor.status(), "File Utility Menu");

    editor.submit("0").unwrap();
    editor.submit("MENU").unwrap();
    assert_eq!(
        editor.submit("ACAD.MNU").unwrap(),
        Effect::LoadMenu("ACAD.MNU".into())
    );
}

#[test]
fn files_dialog_builds_list_delete_and_rename_requests() {
    let mut editor = Editor::default();
    let Effect::Report(menu) = editor.submit("FILES").unwrap() else {
        panic!("FILES should display the utility menu");
    };
    assert!(menu.contains("List Drawing files"));
    assert_eq!(editor.prompt(), "FILES: selection (0–7)");

    editor.submit("1").unwrap();
    assert_eq!(editor.prompt(), "FILES: drive letter (A–Z)");
    assert_eq!(
        editor.submit("b").unwrap(),
        Effect::Files(FilesRequest::ListDrive {
            filter: FilesFilter::Drawings,
            drive: 'B',
        })
    );

    editor.submit("6").unwrap();
    assert_eq!(editor.prompt(), "FILES: file deletion specification");
    assert_eq!(
        editor.submit("B:*.DWG").unwrap(),
        Effect::Files(FilesRequest::Delete("B:*.DWG".into()))
    );

    editor.submit("7").unwrap();
    editor.submit("B:OLD.DWG").unwrap();
    assert_eq!(editor.prompt(), "FILES: new filename");
    assert_eq!(
        editor.submit("B:NEW.DWG").unwrap(),
        Effect::Files(FilesRequest::Rename {
            source: "B:OLD.DWG".into(),
            destination: "B:NEW.DWG".into(),
        })
    );
    assert_eq!(editor.prompt(), "FILES: selection (0–7)");
    editor.submit("0").unwrap();
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn dim_creates_dimension_geometry_after_the_observed_point_prompt_sequence() {
    let mut editor = Editor::default();
    let before = editor.drawing().clone();
    for (input, prompt, mouse_point) in [
        ("DIM", "DIM: first extension line origin or (ABCT)", true),
        ("1,1", "DIM: dimension line intersection", true),
        ("5,1", "DIM: second extension line origin", true),
        ("3,2", "DIM: dimension text", false),
    ] {
        editor.submit(input).unwrap();
        assert_eq!(editor.prompt(), prompt);
        assert_eq!(editor.accepts_mouse_point(), mouse_point);
        assert!(!editor.accepts_mouse_selection());
    }
    editor.submit("").unwrap();
    assert_eq!(editor.prompt(), "Command");
    assert_ne!(editor.drawing(), &before);
    assert_eq!(editor.drawing().items.len(), 7);
    let entities = editor
        .drawing()
        .items
        .iter()
        .map(|item| match item {
            Item::Entity(Entity::OnLayer { entity, .. }) => entity.as_ref(),
            other => panic!("unexpected DIM output: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(entities[0], Entity::Line { start, end }
        if *start == Point { x: 1.0, y: 1.0 }
            && *end == Point { x: 5.140625, y: 1.0 }));
    assert!(matches!(entities[1], Entity::Line { start, end }
        if *start == Point { x: 3.0, y: 2.0 }
            && *end == Point { x: 5.140625, y: 2.0 }));
    assert!(matches!(entities[4], Entity::Solid { p3, p4, .. }
        if *p3 == Point { x: 5.0, y: 2.0 } && *p4 == *p3));
    assert!(matches!(entities[6], Entity::Text { value, height, .. }
        if value == "1.0000" && *height == 0.2109375));
}

#[test]
fn dim_settings_arrow_is_single_shot_and_drawing_neutral() {
    let mut editor = Editor::default();
    for input in ["LINE", "1,1", "2,2", ""] {
        editor.submit(input).unwrap();
    }
    let before = editor.drawing().clone();
    for answer in ["0.140625", "0.25", "0.5", "2", "0.21875"] {
        editor.submit("DIM").unwrap();
        editor.submit("a").unwrap();
        assert_eq!(editor.prompt(), "DIM: dimension arrow size");
        assert!(!editor.accepts_mouse_point());
        assert!(!editor.accepts_mouse_selection());
        assert!(editor.submit_mouse_point(Point { x: 1.0, y: 2.0 }).is_err());
        assert_eq!(editor.prompt(), "DIM: dimension arrow size");
        editor.submit(answer).unwrap();
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing(), &before);
    }
    for answer in ["", "NaN", "inf", "-inf", "0", "-0", "-1", "0,0", "1'-2\""] {
        editor.submit("DIM").unwrap();
        editor.submit("A").unwrap();
        assert!(editor.submit(answer).is_err(), "{answer:?}");
        assert_eq!(editor.prompt(), "Command", "{answer:?}");
        assert_eq!(editor.drawing(), &before, "{answer:?}");
    }
}

#[test]
fn dim_settings_text_remembers_independent_questions_and_blank_defaults() {
    let mut editor = Editor::default();
    let before = editor.drawing().clone();
    for (inside, outside, inside_prompt, outside_prompt) in [
        (
            "y",
            "n",
            "DIM: inside horizontal text? <Y>",
            "DIM: outside horizontal text? <Y>",
        ),
        (
            "n",
            "",
            "DIM: inside horizontal text? <Y>",
            "DIM: outside horizontal text? <N>",
        ),
        (
            "",
            "",
            "DIM: inside horizontal text? <N>",
            "DIM: outside horizontal text? <N>",
        ),
        (
            "Y",
            "Y",
            "DIM: inside horizontal text? <N>",
            "DIM: outside horizontal text? <N>",
        ),
        (
            "",
            "",
            "DIM: inside horizontal text? <Y>",
            "DIM: outside horizontal text? <Y>",
        ),
    ] {
        editor.submit("DIM").unwrap();
        editor.submit("t").unwrap();
        for (answer, prompt) in [(inside, inside_prompt), (outside, outside_prompt)] {
            assert_eq!(editor.prompt(), prompt);
            assert!(!editor.accepts_mouse_point());
            assert!(!editor.accepts_mouse_selection());
            assert!(editor.submit_mouse_point(Point { x: 1.0, y: 2.0 }).is_err());
            assert_eq!(editor.prompt(), prompt);
            editor.submit(answer).unwrap();
            assert_eq!(editor.drawing(), &before);
        }
        assert_eq!(editor.prompt(), "Command");
    }
}

#[test]
fn dim_settings_text_errors_retry_and_cancellation_discards_pending_answers() {
    let mut editor = Editor::default();
    let before = editor.drawing().clone();
    for menu_cancel in [false, true] {
        editor.submit("DIM").unwrap();
        editor.submit("T").unwrap();
        for answer in ["YES", "ON", "1", "bad"] {
            assert!(editor.submit(answer).is_err());
            assert_eq!(editor.prompt(), "DIM: inside horizontal text? <Y>");
            assert_eq!(editor.drawing(), &before);
        }
        editor.submit("N").unwrap();
        assert!(editor.submit("NO").is_err());
        assert_eq!(editor.prompt(), "DIM: outside horizontal text? <Y>");
        if menu_cancel {
            editor
                .apply_menu_control(acad_cmd::MenuControl::Cancel)
                .unwrap();
        } else {
            editor.cancel_command().unwrap();
        }
        assert_eq!(editor.prompt(), "Command");
        editor.submit("DIM").unwrap();
        editor.submit("T").unwrap();
        assert_eq!(editor.prompt(), "DIM: inside horizontal text? <Y>");
        editor.submit("").unwrap();
        assert_eq!(editor.prompt(), "DIM: outside horizontal text? <Y>");
        editor.submit("").unwrap();
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn hatch_lists_patterns_and_creates_clipped_line_pattern_blocks() {
    let mut editor = Editor::default();
    editor.submit("HATCH").unwrap();
    assert_eq!(editor.prompt(), "HATCH: pattern (name,style / U / ?)");
    let Effect::Report(patterns) = editor.submit("?").unwrap() else {
        panic!("HATCH ? should show its pattern list");
    };
    assert!(patterns.contains("LINE            - Parallel horizontal lines"));
    assert!(patterns.contains("ZIGZAG          - Staircase effect"));
    assert_eq!(editor.prompt(), "Command");

    editor.submit("HATCH").unwrap();
    assert!(editor
        .submit("ANSI31")
        .unwrap_err()
        .contains("unknown HATCH pattern"));
    assert_eq!(editor.prompt(), "Command");

    for input in ["LINE", "1,1", "5,1", "5,5", "1,5", "1,1", ""] {
        editor.submit(input).unwrap();
    }

    editor.submit("HATCH").unwrap();
    editor.submit("LINE").unwrap();
    assert_eq!(editor.prompt(), "HATCH: scale for pattern {1}");
    editor.submit("").unwrap();
    assert_eq!(editor.prompt(), "HATCH: angle for pattern {0}");
    editor.submit("").unwrap();
    assert_eq!(editor.prompt(), "HATCH: select objects on Window or Last");
    assert!(editor.accepts_mouse_selection());
    editor.submit("W").unwrap();
    assert_eq!(editor.prompt(), "HATCH: lower left corner");
    assert!(editor.accepts_mouse_point());
    editor.submit("0,0").unwrap();
    assert_eq!(editor.prompt(), "HATCH: upper right corner");
    editor.submit("6,6").unwrap();
    assert_eq!(editor.prompt(), "Command");
    let Item::Block(hatch) = &editor.drawing().items[4] else {
        panic!("HATCH should store its clipped pattern in a block");
    };
    assert_eq!(hatch.name, "*X1");
    assert_eq!(hatch.entities.len(), 32);
    let Entity::OnLayer { layer: 127, entity } = &hatch.entities[0] else {
        panic!("pattern strokes use layer 127");
    };
    assert!(matches!(entity.as_ref(), Entity::Line { start, end }
        if *start == Point { x: 1.0, y: 3.0 } && *end == Point { x: 5.0, y: 3.0 }));
    assert!(
        matches!(&editor.drawing().items[5], Item::Entity(Entity::OnLayer {
        layer: 1,
        entity,
    }) if matches!(entity.as_ref(), Entity::Insert { name, .. } if name == "*X1"))
    );

    editor.drawing_mut().items.clear();
    editor.submit("SKETCH").unwrap();
    assert_eq!(editor.prompt(), "SKETCH: record increment");
    assert!(editor.submit("0").unwrap_err().contains("must be positive"));
    assert_eq!(editor.prompt(), "SKETCH: record increment");
    editor.submit("0.5").unwrap();
    assert_eq!(
        editor.prompt(),
        "Sketch.  Pen eXit Quit Record Erase Connect ."
    );
    editor.submit("Q").unwrap();
    assert_eq!(editor.prompt(), "Command");
    assert!(editor.drawing().items.is_empty());
}

#[test]
fn hatch_rejects_an_open_boundary_without_adding_pattern_geometry() {
    let mut editor = Editor::default();
    for input in [
        "LINE", "1,1", "5,1", "5,5", "1,5", "", "HATCH", "LINE", "", "", "W", "0,0",
    ] {
        editor.submit(input).unwrap();
    }
    let before = editor.drawing().items.clone();
    assert!(editor
        .submit("6,6")
        .unwrap_err()
        .contains("closed, unbranched loops"));
    assert_eq!(editor.drawing().items, before);
    assert!(editor.prompt().starts_with("Select objects:"));
}

#[test]
fn hatch_line_pattern_clips_circle_chords_without_emitting_tangents() {
    let mut editor = Editor::default();
    for input in [
        "CIRCLE", "3,3", "2", "HATCH", "LINE", "", "", "W", "0,0", "6,6",
    ] {
        editor.submit(input).unwrap();
    }
    let Item::Block(hatch) = &editor.drawing().items[1] else {
        panic!("HATCH should store circle chords in a block");
    };
    assert_eq!(hatch.entities.len(), 31);
    assert!(matches!(&hatch.entities[0], Entity::OnLayer {
        layer: 127,
        entity,
    } if matches!(entity.as_ref(), Entity::Line { start, end }
        if start.x == 1.0 && start.y == 3.0 && end.x == 5.0 && end.y == 3.0)));
    assert!(matches!(&hatch.entities[1], Entity::OnLayer {
        layer: 127,
        entity,
    } if matches!(entity.as_ref(), Entity::Line { start, end }
        if start.y == 3.125 && end.y == 3.125 && start.x > 1.0 && end.x < 5.0)));
}

#[test]
fn oops_restores_the_last_erased_items_in_place_and_undo_reverses_it() {
    let mut editor = Editor::default();
    editor.drawing_mut().items = vec![
        Item::Entity(Entity::Point {
            origin: Point { x: 1.0, y: 0.0 },
        }),
        Item::Block(acad_model::Block {
            name: "KEEP".into(),
            base: Point { x: 0.0, y: 0.0 },
            entities: vec![],
        }),
        Item::Entity(Entity::Point {
            origin: Point { x: 2.0, y: 0.0 },
        }),
        Item::Entity(Entity::Load { name: "TXT".into() }),
        Item::Entity(Entity::Point {
            origin: Point { x: 3.0, y: 0.0 },
        }),
    ];
    let before = editor.drawing().items.clone();
    editor.submit("ERASE").unwrap();
    editor.submit("1,3").unwrap();
    assert_eq!(editor.drawing().items.len(), 5);
    assert_eq!(editor.drawing().entities().count(), 2);
    assert!(matches!(editor.drawing().items[0], Item::Erased(_)));
    assert!(matches!(editor.drawing().items[4], Item::Erased(_)));

    editor.submit("OOPS").unwrap();
    assert_eq!(editor.drawing().items, before);
    assert_eq!(editor.status(), "Restored 2 erased entities");

    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing().items.len(), 5);
    assert_eq!(editor.drawing().entities().count(), 2);
    editor.submit("OOPS").unwrap();
    assert_eq!(editor.drawing().items, before);
}

#[test]
fn array_rejects_zero_dimensions_and_overflow_before_creating_copies() {
    let mut editor = Editor::default();
    for input in ["POINT", "2,3", "ARRAY", "ALL", "R"] {
        editor.submit(input).unwrap();
    }
    assert!(editor.submit("0").is_err());
    assert_eq!(editor.prompt(), "ARRAY: number of rows");
    editor.submit("2").unwrap();
    assert!(editor.submit(usize::MAX.to_string().as_str()).is_err());
    assert!(editor.submit("100001").is_err());
    assert_eq!(editor.prompt(), "ARRAY: number of columns");
    assert_eq!(editor.drawing().entities().count(), 1);
}

#[test]
fn change_point_moves_the_nearest_line_endpoint_and_undo_restores_it() {
    let mut editor = Editor::default();
    for input in ["LINE", "1,1", "2,1", "", "CHANGE", "1", "3,4"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(
        editor.drawing().entities().next().unwrap(),
        &Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Line {
                start: Point { x: 1.0, y: 1.0 },
                end: Point { x: 3.0, y: 4.0 },
            }),
        }
    );
    editor.submit("UNDO").unwrap();
    assert_eq!(
        editor.drawing().entities().next().unwrap(),
        &Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Line {
                start: Point { x: 1.0, y: 1.0 },
                end: Point { x: 2.0, y: 1.0 },
            }),
        }
    );
}

#[test]
fn fillet_rejects_parallel_lines_and_excessive_radius_without_mutation() {
    let mut editor = Editor::default();
    for input in ["LINE", "0,0", "10,0", "", "LINE", "0,1", "10,1", ""] {
        editor.submit(input).unwrap();
    }
    let original = editor.drawing().items.clone();
    for input in ["FILLET", "R", "1", "FILLET"] {
        editor.submit(input).unwrap();
    }
    assert!(editor.submit("1,2").is_err());
    assert_eq!(editor.drawing().items, original);
    assert_eq!(editor.prompt(), "FILLET: select two LINE entities or R");

    editor = Editor::default();
    for input in ["LINE", "-5,0", "5,0", "", "LINE", "0,-5", "0,5", ""] {
        editor.submit(input).unwrap();
    }
    for input in ["FILLET", "R", "100", "FILLET"] {
        editor.submit(input).unwrap();
    }
    assert!(editor.submit("1,2").is_err());
    assert_eq!(editor.drawing().items.len(), 2);
}

fn line_points(entity: &Entity) -> Option<(Point, Point)> {
    match entity {
        Entity::OnLayer { entity, .. } => line_points(entity),
        Entity::Line { start, end } => Some((*start, *end)),
        _ => None,
    }
}

#[test]
fn break_projects_off_line_points_and_cuts_an_end_at_an_endpoint() {
    // In-tree original: `7,1` on LINE 0,0-10,0 cuts at 7,0 (docs/native-array-break.md).
    let mut editor = Editor::default();
    for input in ["LINE", "0,0", "10,0", ""] {
        editor.submit(input).unwrap();
    }
    for input in ["BREAK", "1", "2,1"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.prompt(), "BREAK: second point");
    editor.submit("7,0").unwrap();
    let lines: Vec<_> = editor
        .drawing()
        .entities()
        .filter_map(line_points)
        .collect();
    assert_eq!(
        lines,
        [
            (Point { x: 0.0, y: 0.0 }, Point { x: 2.0, y: 0.0 }),
            (Point { x: 7.0, y: 0.0 }, Point { x: 10.0, y: 0.0 })
        ]
    );

    let mut endpoint_editor = Editor::default();
    for input in ["LINE", "0,0", "10,0", "", "BREAK", "1", "0,0", "5,0"] {
        endpoint_editor.submit(input).unwrap();
    }
    let lines: Vec<_> = endpoint_editor
        .drawing()
        .entities()
        .filter_map(line_points)
        .collect();
    assert_eq!(
        lines,
        [(Point { x: 5.0, y: 0.0 }, Point { x: 10.0, y: 0.0 })]
    );
    endpoint_editor.submit("UNDO").unwrap();
    assert_eq!(endpoint_editor.drawing().items.len(), 1);
}

#[test]
fn break_circle_projects_points_radially_and_rejects_the_center() {
    let mut editor = Editor::default();
    for input in ["CIRCLE", "1,2", "3", "BREAK", "1", "4,2"] {
        editor.submit(input).unwrap();
    }
    let original = editor.drawing().items.clone();
    assert!(editor.submit("1,2").is_err(), "the center has no angle");
    assert_eq!(editor.drawing().items, original);
    assert_eq!(editor.prompt(), "BREAK: second point");
    editor.submit("1,4").unwrap();
    let Some(Entity::Arc {
        start_deg, end_deg, ..
    }) = editor.drawing().entities().next().map(|e| match e {
        Entity::OnLayer { entity, .. } => entity.as_ref(),
        other => other,
    })
    else {
        panic!("BREAK leaves an arc");
    };
    assert!((start_deg - 90.0).abs() < 1e-9 && end_deg.abs() < 1e-9);
}

#[test]
fn break_arc_snaps_a_point_outside_its_sweep_to_the_nearer_end() {
    let mut editor = Editor::default();
    editor.drawing_mut().items.push(Item::Entity(Entity::Arc {
        center: Point { x: 0.0, y: 0.0 },
        radius: 2.0,
        start_deg: 20.0,
        end_deg: 160.0,
    }));
    for input in ["BREAK", "1", "0,2", "-2,0"] {
        editor.submit(input).unwrap();
    }
    let arcs: Vec<_> = editor
        .drawing()
        .entities()
        .map(|e| match e {
            Entity::OnLayer { entity, .. } => entity.as_ref().clone(),
            other => other.clone(),
        })
        .collect();
    let [Entity::Arc {
        start_deg, end_deg, ..
    }] = arcs.as_slice()
    else {
        panic!("one retained arc: {arcs:?}");
    };
    assert_eq!(*start_deg, 20.0);
    assert!((end_deg - 90.0).abs() < 1e-9);
}

#[test]
fn distance_reports_original_four_decimal_length() {
    let mut editor = Editor::default();
    for input in ["DIST", "1,2", "4,6"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.status(), "Distance=5.0000");
    assert_eq!(editor.drawing().entities().count(), 0);
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn id_reports_coordinates_without_changing_the_drawing() {
    let mut editor = Editor::default();
    let before = editor.drawing().clone();
    editor.submit("ID").unwrap();
    assert_eq!(editor.prompt(), "ID: point");
    editor.submit("3,4").unwrap();
    assert_eq!(editor.status(), "X = 3.0000    Y = 4.0000");
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing(), &before);
}

#[test]
fn block_rejects_duplicate_name_and_invalid_selection_without_mutation() {
    let mut editor = Editor::default();
    for input in ["POINT", "1,2", "BLOCK", "SYMBOL", "0,0", "LAST"] {
        editor.submit(input).unwrap();
    }
    let after_first = editor.drawing().clone();
    editor.submit("BLOCK").unwrap();
    assert!(editor.submit("symbol").is_err());
    assert_eq!(editor.drawing(), &after_first);
    editor.submit("OTHER").unwrap();
    editor.submit("0,0").unwrap();
    assert!(editor.submit("LAST").is_err());
    assert_eq!(editor.drawing(), &after_first);
}

#[test]
fn solid_chains_the_prior_third_and_fourth_points() {
    let mut editor = Editor::default();
    for input in ["SOLID", "1,1", "4,1", "1,3", "4,3", "1,5", "4,5", ""] {
        editor.submit(input).unwrap();
    }
    let solids: Vec<_> = editor.drawing().entities().cloned().collect();
    assert_eq!(
        solids,
        [
            Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Solid {
                    p1: Point { x: 1.0, y: 1.0 },
                    p2: Point { x: 4.0, y: 1.0 },
                    p3: Point { x: 1.0, y: 3.0 },
                    p4: Point { x: 4.0, y: 3.0 },
                }),
            },
            Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Solid {
                    p1: Point { x: 1.0, y: 3.0 },
                    p2: Point { x: 4.0, y: 3.0 },
                    p3: Point { x: 1.0, y: 5.0 },
                    p4: Point { x: 4.0, y: 5.0 },
                }),
            },
        ]
    );
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn trace_miters_a_bend_and_updates_saved_width() {
    let mut editor = Editor::default();
    for input in ["TRACE", "0.5", "1,1", "4,1", "4,4", ""] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().header.trace_width, 0.5);
    let traces: Vec<_> = editor.drawing().entities().cloned().collect();
    assert_eq!(
        traces,
        [
            Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Trace {
                    p1: Point { x: 1.0, y: 1.25 },
                    p2: Point { x: 1.0, y: 0.75 },
                    p3: Point { x: 3.75, y: 1.25 },
                    p4: Point { x: 4.25, y: 0.75 },
                }),
            },
            Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Trace {
                    p1: Point { x: 3.75, y: 1.25 },
                    p2: Point { x: 4.25, y: 0.75 },
                    p3: Point { x: 3.75, y: 4.0 },
                    p4: Point { x: 4.25, y: 4.0 },
                }),
            },
        ]
    );
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn area_follows_original_point_sequence_and_reports_four_decimals() {
    let mut editor = Editor::default();
    for input in ["AREA", "0,0", "4,0", "4,3"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.prompt(), "AREA: next point (Enter to finish)");
    editor.submit("").unwrap();
    assert_eq!(editor.status(), "Area = 6.0000");
    assert_eq!(editor.prompt(), "Command");
    assert!(editor.drawing().items.is_empty());
}

#[test]
fn entity_area_measures_circles_and_reordered_closed_line_loops() {
    let mut circle_editor = Editor::default();
    for input in ["CIRCLE", "1,2", "3", "ENTITYAREA", "1"] {
        circle_editor.submit(input).unwrap();
    }
    assert_eq!(
        circle_editor.status(),
        "Area=28.274334, Perimeter=18.849556"
    );

    let mut polygon = Editor::default();
    for (start, end) in [
        ("4,3", "4,0"),
        ("0,0", "4,0"),
        ("0,3", "0,0"),
        ("4,3", "0,3"),
    ] {
        for input in ["LINE", start, end, ""] {
            polygon.submit(input).unwrap();
        }
    }
    polygon.submit("ENTITYAREA").unwrap();
    polygon.submit("ALL").unwrap();
    assert_eq!(polygon.status(), "Area=12.000000, Perimeter=14.000000");
}

#[test]
fn entity_area_rejects_an_open_line_selection_without_mutating_it() {
    let mut editor = Editor::default();
    for (start, end) in [("0,0", "4,0"), ("4,0", "4,3")] {
        for input in ["LINE", start, end, ""] {
            editor.submit(input).unwrap();
        }
    }
    let original = editor.drawing().items.clone();
    editor.submit("ENTITYAREA").unwrap();
    assert!(editor.submit("ALL").is_err());
    assert_eq!(editor.drawing().items, original);
    assert_eq!(editor.prompt(), "ENTITYAREA: entity numbers or ALL");
}

#[test]
fn wblock_star_exports_live_entities_and_reachable_blocks_without_changing_source() {
    let mut editor = Editor::default();
    for input in [
        "LINE",
        "0,0",
        "1,0",
        "",
        "BLOCK",
        "USED",
        "0,0",
        "LAST",
        "LINE",
        "2,0",
        "3,0",
        "",
        "BLOCK",
        "UNUSED",
        "2,0",
        "LAST",
        "INSERT",
        "USED",
        "5,5",
        "",
        "",
        "",
        "",
        "LINE",
        "4,0",
        "5,0",
        "",
        "BLOCK",
        "ERASEDONLY",
        "4,0",
        "LAST",
        "INSERT",
        "ERASEDONLY",
        "7,7",
        "",
        "",
        "",
        "ERASE",
        "LAST",
    ] {
        editor.submit(input).unwrap();
    }
    let source = editor.drawing().clone();
    editor.submit("WBLOCK").unwrap();
    assert_eq!(editor.prompt(), "WBLOCK: output file name");
    editor.submit("whole").unwrap();
    assert_eq!(editor.prompt(), "WBLOCK: block name (* for entire drawing)");
    let effect = editor.submit("*").unwrap();
    let Effect::SaveDrawing(path, exported) = effect else {
        panic!("expected WBLOCK to return a separate drawing snapshot");
    };
    assert_eq!(path, "whole.DWG");
    assert_eq!(editor.drawing(), &source, "WBLOCK must not edit the source");
    assert_eq!(exported.header, source.header);
    assert_eq!(
        exported.items,
        vec![
            Item::Block(acad_model::Block {
                name: "USED".into(),
                base: Point { x: 0.0, y: 0.0 },
                entities: vec![Entity::OnLayer {
                    layer: 1,
                    entity: Box::new(Entity::Line {
                        start: Point { x: 0.0, y: 0.0 },
                        end: Point { x: 1.0, y: 0.0 },
                    }),
                }],
            }),
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Insert {
                    origin: Point { x: 5.0, y: 5.0 },
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotation_deg: 0.0,
                    name: "USED".into(),
                }),
            }),
        ]
    );
}

#[test]
fn wblock_blank_block_name_exports_only_selected_entities_and_base_point() {
    let mut editor = Editor::default();
    for input in ["LINE", "0,0", "1,0", "", "LINE", "2,0", "3,0", ""] {
        editor.submit(input).unwrap();
    }
    let source = editor.drawing().clone();
    for (input, prompt) in [
        ("WBLOCK", "WBLOCK: output file name"),
        ("selected", "WBLOCK: block name (* for entire drawing)"),
        ("", "WBLOCK: insertion base point"),
        ("1,2", "WBLOCK: entity numbers, ALL, or LAST"),
    ] {
        editor.submit(input).unwrap();
        assert_eq!(editor.prompt(), prompt);
    }
    let effect = editor.submit("2").unwrap();
    let Effect::SaveDrawing(path, drawing) = effect else {
        panic!("selected WBLOCK should return a separate drawing snapshot");
    };
    assert_eq!(path, "selected.DWG");
    assert_eq!(drawing.header.base, Point { x: 1.0, y: 2.0 });
    assert_eq!(drawing.items, vec![source.items[1].clone()]);
    assert_eq!(editor.drawing(), &source, "WBLOCK must not edit the source");
}

#[test]
fn resolution_aliases_share_snap_state_and_delay_and_resume_do_not_edit() {
    let mut editor = Editor::default();
    editor.submit("RES").unwrap();
    assert_eq!(editor.prompt(), "SNAP: spacing");
    editor.submit("2.5").unwrap();
    assert_eq!(
        editor.drawing().header.snap,
        acad_model::Mode {
            on: true,
            spacing: 2.5
        }
    );
    editor.submit("RESOLUTION").unwrap();
    editor.submit("OFF").unwrap();
    assert_eq!(
        editor.drawing().header.snap,
        acad_model::Mode {
            on: false,
            spacing: 2.5
        }
    );
    let before = editor.drawing().clone();
    editor.submit("DELAY").unwrap();
    assert_eq!(editor.prompt(), "DELAY: duration");
    editor.submit("0").unwrap();
    editor.submit("RESUME").unwrap();
    assert_eq!(editor.drawing(), &before);
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn units_persists_the_native_choice_and_validates_its_precision() {
    let mut editor = Editor::default();
    editor.submit("UNITS").unwrap();
    assert_eq!(editor.prompt(), "UNITS: choice, 1 to 4");
    editor.submit("4").unwrap();
    assert_eq!(
        editor.prompt(),
        "UNITS: denominator (1, 2, 4, 8, 16, 32, or 64)"
    );
    editor.submit("16").unwrap();
    assert_eq!(
        editor.drawing().header.units,
        Units {
            format: UnitFormat::Architectural,
            precision: 16
        }
    );
    for input in ["DIST", "0,0", "1'-3 1/2\",0"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.status(), "Distance=1'-3 1/2\"");
    editor.submit("UNITS").unwrap();
    editor.submit("2").unwrap();
    assert!(editor.submit("9").is_err());
    assert_eq!(
        editor.prompt(),
        "UNITS: digits to right of decimal point (0 to 8)"
    );
    editor.submit("3").unwrap();
    assert_eq!(
        editor.drawing().header.units,
        Units {
            format: UnitFormat::Decimal,
            precision: 3
        }
    );
}

#[test]
fn invalid_selection_does_not_change_the_drawing() {
    let mut editor = Editor::default();
    for input in ["POINT", "1,2", "ERASE"] {
        editor.submit(input).unwrap();
    }
    assert!(editor.submit("2").is_err());
    assert_eq!(editor.drawing().entities().count(), 1);
    assert_eq!(editor.prompt(), "ERASE: entity numbers or ALL");
}

#[test]
fn editing_geometry_preserves_the_user_view_and_limits() {
    let mut editor = Editor::default();
    for input in ["LINE", "0,0", "10,0", ""] {
        editor.submit(input).unwrap();
    }
    for input in ["ZOOM", "2", "LIMITS", "-50,-40", "60,70"] {
        editor.submit(input).unwrap();
    }
    let view = editor.drawing().header.view;
    let limits = editor.drawing().header.limits;
    for input in ["MOVE", "0,0", "1,0", "ALL"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().header.view, view);
    assert_eq!(editor.drawing().header.limits, limits);
    assert_eq!(editor.drawing().header.extents.xmin, 1.0);
    assert_eq!(editor.drawing().header.extents.xmax, 11.0);
}

#[test]
fn adding_geometry_preserves_view_and_limits_and_undo_restores_extents() {
    let mut editor = Editor::default();
    for input in ["ZOOM", "C", "-2,3", "20", "LIMITS", "-50,-40", "60,70"] {
        editor.submit(input).unwrap();
    }
    let original = editor.drawing().clone();
    for input in ["LINE", "1,2", "11,7", ""] {
        editor.submit(input).unwrap();
        assert_eq!(editor.drawing().header.view, original.header.view);
        assert_eq!(editor.drawing().header.limits, original.header.limits);
    }
    assert_eq!(editor.drawing().header.extents.xmin, 1.0);
    assert_eq!(editor.drawing().header.extents.xmax, 11.0);
    assert_eq!(editor.drawing().header.extents.ymin, 2.0);
    assert_eq!(editor.drawing().header.extents.ymax, 7.0);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), &original);
    for input in ["CIRCLE", "4,5", "2"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.drawing().header.view, original.header.view);
    assert_eq!(editor.drawing().header.limits, original.header.limits);
    assert_eq!(editor.drawing().header.extents.xmin, 2.0);
    assert_eq!(editor.drawing().header.extents.xmax, 6.0);
}

fn menu_control_editor() -> Editor {
    let mut editor = Editor::default();
    editor.drawing_mut().header.snap.spacing = 0.5;
    editor
}

fn menu_control_apply(editor: &mut Editor, control: acad_cmd::MenuControl, status: &str) {
    assert_eq!(editor.apply_menu_control(control), Ok(Effect::Continue));
    assert_eq!(editor.status(), status);
}

#[test]
fn menu_control_snap_idle_matches_native() {
    let mut editor = menu_control_editor();
    let mut expected = editor.drawing().clone();
    expected.header.snap.on = true;
    menu_control_apply(&mut editor, acad_cmd::MenuControl::Snap, "<Snap on>");
    assert_eq!(editor.drawing(), &expected);
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn menu_control_ortho_idle_matches_native() {
    let mut editor = menu_control_editor();
    let mut expected = editor.drawing().clone();
    expected.header.ortho = true;
    menu_control_apply(&mut editor, acad_cmd::MenuControl::Ortho, "<Ortho on>");
    assert_eq!(editor.drawing(), &expected);
}

#[test]
fn menu_control_twice_and_initial_on_match_native() {
    for (control, on, off) in [
        (acad_cmd::MenuControl::Snap, "<Snap on>", "<Snap off>"),
        (acad_cmd::MenuControl::Ortho, "<Ortho on>", "<Ortho off>"),
    ] {
        let mut editor = menu_control_editor();
        let original = editor.drawing().clone();
        menu_control_apply(&mut editor, control, on);
        menu_control_apply(&mut editor, control, off);
        assert_eq!(editor.drawing(), &original);
        editor.drawing_mut().header.snap.on = true;
        editor.drawing_mut().header.ortho = true;
        let mut expected = editor.drawing().clone();
        match control {
            acad_cmd::MenuControl::Snap => expected.header.snap.on = false,
            acad_cmd::MenuControl::Ortho => expected.header.ortho = false,
            acad_cmd::MenuControl::Cancel => unreachable!(),
        }
        menu_control_apply(&mut editor, control, off);
        assert_eq!(editor.drawing(), &expected);
    }
}

#[test]
fn menu_control_preserves_fractional_line_pending_state() {
    for control in [acad_cmd::MenuControl::Snap, acad_cmd::MenuControl::Ortho] {
        let mut editor = menu_control_editor();
        editor.submit("LINE").unwrap();
        editor.submit("2.1,3.15").unwrap();
        editor.apply_menu_control(control).unwrap();
        assert_eq!(editor.prompt(), "LINE: next point (Enter to finish)");
        assert!(editor.drawing().items.is_empty());
        editor.submit("4.25,5.15").unwrap();
        editor.submit("").unwrap();
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(
            bare_first(&editor),
            &Entity::Line {
                start: Point { x: 2.1, y: 3.15 },
                end: Point { x: 4.25, y: 5.15 },
            }
        );
        assert_eq!(editor.drawing().items.len(), 1);
    }
}

#[test]
fn menu_control_preserves_circle_radius_pending_state() {
    for control in [acad_cmd::MenuControl::Snap, acad_cmd::MenuControl::Ortho] {
        let mut editor = menu_control_editor();
        editor.submit("CIRCLE").unwrap();
        editor.submit("2,3").unwrap();
        editor.apply_menu_control(control).unwrap();
        assert_eq!(editor.prompt(), "CIRCLE: radius or point (D for diameter)");
        assert!(editor.drawing().items.is_empty());
        editor.submit("1.25").unwrap();
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(
            bare_first(&editor),
            &Entity::Circle {
                center: Point { x: 2.0, y: 3.0 },
                radius: 1.25,
            }
        );
        assert_eq!(editor.drawing().items.len(), 1);
    }
}

#[test]
fn menu_control_cancel_retains_completed_segment() {
    let mut editor = menu_control_editor();
    for input in ["LINE", "2,3", "4,5"] {
        editor.submit(input).unwrap();
    }
    let completed = editor.drawing().clone();
    menu_control_apply(&mut editor, acad_cmd::MenuControl::Cancel, "*Cancel*");
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing(), &completed);
    editor.submit("POINT").unwrap();
    editor.submit("8,7").unwrap();
    assert_eq!(&editor.drawing().items[..1], completed.items);
    assert_eq!(editor.drawing().items.len(), 2);
    assert_eq!(
        editor.drawing().entities().last().unwrap(),
        &Entity::OnLayer {
            layer: 1,
            entity: Box::new(Entity::Point {
                origin: Point { x: 8.0, y: 7.0 }
            }),
        }
    );
}

#[test]
fn menu_control_cancel_discards_only_pending_geometry() {
    for setup in [
        vec![],
        vec!["LINE"],
        vec!["LINE", "2,3"],
        vec!["CIRCLE", "2,3"],
        vec!["MENU"],
    ] {
        let mut editor = menu_control_editor();
        for input in setup {
            editor.submit(input).unwrap();
        }
        let original = editor.drawing().clone();
        menu_control_apply(&mut editor, acad_cmd::MenuControl::Cancel, "*Cancel*");
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing(), &original);
        editor.submit("POINT").unwrap();
        editor.submit("8,7").unwrap();
        assert_eq!(editor.drawing().items.len(), 1);
        assert_eq!(
            bare_first(&editor),
            &Entity::Point {
                origin: Point { x: 8.0, y: 7.0 }
            }
        );
    }
}

#[test]
fn menu_control_cancel_retains_repeat_marker() {
    for control in [
        acad_cmd::MenuControl::Cancel,
        acad_cmd::MenuControl::Snap,
        acad_cmd::MenuControl::Ortho,
    ] {
        let mut editor = menu_control_editor();
        for input in ["REPEAT", "POINT", "4,5"] {
            editor.submit(input).unwrap();
        }
        editor.apply_menu_control(control).unwrap();
        editor.submit("ENDREP").unwrap();
        assert_eq!(editor.prompt(), "ENDREP: columns");
        assert_eq!(editor.drawing().items.len(), 1);
        assert_eq!(
            bare_first(&editor),
            &Entity::Point {
                origin: Point { x: 4.0, y: 5.0 }
            }
        );
    }
}

const RETURN_UNKNOWN: &str = "Unknown command. Type ? for list of commands.";

#[test]
fn menu_return_fresh_unknown_and_semicolon_are_distinct() {
    let mut editor = Editor::default();
    assert_eq!(editor.submit(""), Ok(Effect::Continue));
    for input in ["", ";"] {
        assert_eq!(editor.submit_return(input), Err(RETURN_UNKNOWN.into()));
        assert_eq!(editor.status(), RETURN_UNKNOWN);
        assert_eq!(editor.prompt(), "Command");
        assert!(editor.drawing().items.is_empty());
    }
    // Raw macro commands and their empty pieces do not acquire Return history.
    for input in ["POINT", "8,7", ""] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.submit_return(""), Err(RETURN_UNKNOWN.into()));
    assert_eq!(editor.drawing().items.len(), 1);
    editor.submit("MENU").unwrap();
    assert_eq!(editor.submit(""), Ok(Effect::Continue));
    editor.submit("ERASE").unwrap();
    assert_eq!(editor.submit(""), Err("invalid entity number: ".into()));
    assert!(editor.accepts_mouse_selection());
}

#[test]
fn menu_return_empty_active_prompts_match_native() {
    for (setup, status) in [
        (vec!["LINE"], "*Invalid*"),
        (vec!["CIRCLE", "2,3"], "*Invalid*"),
        (vec!["LINE", "2,3"], ""),
        (vec!["LINE", "2,3", "4,5"], ""),
        (vec!["LINE", "2,3", "4,5", "", "ERASE"], ""),
    ] {
        let mut editor = Editor::default();
        for input in setup {
            editor.submit_return(input).unwrap();
        }
        let items = editor.drawing().items.clone();
        assert_eq!(editor.submit_return(""), Ok(Effect::Continue));
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.status(), status);
        assert_eq!(editor.drawing().items, items);
    }
    let mut editor = Editor::default();
    editor.submit_return("MENU").unwrap();
    assert_eq!(editor.submit_return(""), Ok(Effect::UnloadMenu));
    assert_eq!(editor.status(), "");
    assert_eq!(editor.prompt(), "Command");
    // No correction to unobserved nonempty parser errors or other prompts.
    editor.submit_return("CIRCLE").unwrap();
    assert!(editor.submit_return("").is_err());
    assert_eq!(editor.prompt(), "CIRCLE: center point (or 2P/3P)");
    editor.submit_return("2,3").unwrap();
    assert!(editor.submit_return("bad").is_err());
    assert_eq!(editor.prompt(), "CIRCLE: radius or point (D for diameter)");
}

#[test]
fn menu_return_known_history_survives_unknowns_and_prompt_answers() {
    for setup in [vec!["LINE", ""], vec!["LINE", "2,3", ""]] {
        let mut editor = Editor::default();
        for input in setup {
            editor.submit_return(input).unwrap();
        }
        for input in ["2,3", "4,5"] {
            assert_eq!(editor.submit_return(input), Err(RETURN_UNKNOWN.into()));
        }
        editor
            .apply_menu_control(acad_cmd::MenuControl::Snap)
            .unwrap();
        editor
            .apply_menu_control(acad_cmd::MenuControl::Cancel)
            .unwrap();
        editor.submit_return("").unwrap();
        assert_eq!(editor.prompt(), "LINE: first point");
        assert!(editor.drawing().items.is_empty());
    }
    let mut editor = Editor::default();
    editor.submit_return(" point ").unwrap();
    editor.submit_return("8,7").unwrap();
    let items = editor.drawing().items.clone();
    editor.submit_return("").unwrap();
    assert_eq!(editor.prompt(), "POINT: point");
    assert_eq!(editor.drawing().items, items);
    editor
        .apply_menu_control(acad_cmd::MenuControl::Cancel)
        .unwrap();
    editor.submit("LINE").unwrap();
    editor.submit("").unwrap();
    editor.submit_return("").unwrap();
    assert_eq!(editor.prompt(), "POINT: point");
}

#[test]
fn menu_return_invalid_repeat_columns_retains_marker() {
    for invalid in ["REPEAT", "0", "-1", "1.5", "65536"] {
        let mut editor = Editor::default();
        for input in ["REPEAT", "POINT", "4,5"] {
            editor.submit_return(input).unwrap();
        }
        editor
            .apply_menu_control(acad_cmd::MenuControl::Cancel)
            .unwrap();
        editor.submit_return("ENDREP").unwrap();
        let items = editor.drawing().items.clone();
        assert_eq!(editor.submit_return(invalid), Ok(Effect::Continue));
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.status(), "*Invalid*");
        assert_eq!(editor.drawing().items, items);
        for input in ["POINT", "6,5", "ENDREP"] {
            editor.submit_return(input).unwrap();
        }
        assert_eq!(editor.prompt(), "ENDREP: columns");
        assert_eq!(editor.drawing().items.len(), 2);
    }
}

#[test]
fn empty_repeat_creates_empty_group_interactively() {
    let mut editor = Editor::default();
    for input in ["REPEAT", "ENDREP", "2", "1", "5", "0"] {
        editor.submit(input).unwrap();
    }
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing().items.len(), 1);
    match &editor.drawing().items[0] {
        Item::Repeat(repeat) => {
            assert!(repeat.entities.is_empty());
            assert_eq!(repeat.columns, 2);
            assert_eq!(repeat.rows, 1);
            assert_eq!(repeat.column_spacing, 5.0);
            assert_eq!(repeat.row_spacing, 0.0);
        }
        other => panic!("expected Item::Repeat, got {other:?}"),
    }
}
