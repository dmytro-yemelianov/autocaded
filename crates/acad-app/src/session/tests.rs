#[cfg(test)]
use super::*;
use acad_render::{flatten_with_libraries, rasterize, Prim, Viewport};

struct WorkflowDirectory(std::path::PathBuf);

impl WorkflowDirectory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("autocaded workflow {} {id}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for WorkflowDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn return_input(app: &mut Session, input: &str) {
    app.input = input.into();
    app.submit_return_input(&mut |app, result| {
        assert!(result.is_ok(), "{input:?}: {result:?}");
        assert!(!app.apply_result(result));
    });
    assert!(app.input.is_empty());
}

#[test]
fn keyboard_mouse_save_and_reopen_workflow_keeps_geometry_and_view() {
    let directory = WorkflowDirectory::new();
    let input = directory.0.join("source.BAK");
    std::fs::write(
        &input,
        include_bytes!("../../../acad-cmd/tests/fixtures/hatch/HNETDEF.dwg"),
    )
    .unwrap();
    let Some(mut app) = menu_app(0) else {
        return;
    };
    app.editor = acad_cmd::Editor::new(decode_drawing(&std::fs::read(input).unwrap()).unwrap());
    for input in [
        "ZOOM", "C", "6,4", "16", "SNAP", "0.5", "ORTHO", "ON", "LINE",
    ] {
        return_input(&mut app, input);
    }
    let view = app.editor.drawing().header.view;
    let limits = app.editor.drawing().header.limits;
    let before = app.editor.drawing().clone();
    // Use the production pixel route with a stable viewport. SNAP makes the
    // anchor exact, and ORTHO retains its Y for the second mouse point.
    for world in [
        acad_model::Point { x: 6.1, y: 2.1 },
        acad_model::Point { x: 8.1, y: 3.2 },
    ] {
        let screen = viewport_for(app.editor.drawing(), 800, 600).to_screen(world);
        app.cursor = Some((screen.x, screen.y));
        app.handle_left_click(800, 600, |app, result| {
            assert!(!app.apply_result(result));
        });
        assert_eq!(app.editor.drawing().header.view, view);
        assert_eq!(app.editor.drawing().header.limits, limits);
    }
    return_input(&mut app, "");
    let line = app.editor.drawing().items.last().unwrap().clone();
    assert_eq!(
        line,
        acad_model::Item::Entity(acad_model::Entity::OnLayer {
            layer: app.editor.drawing().header.current_layer,
            entity: Box::new(acad_model::Entity::Line {
                start: acad_model::Point { x: 6.0, y: 2.0 },
                end: acad_model::Point { x: 8.0, y: 2.0 },
            }),
        })
    );
    return_input(&mut app, "UNDO");
    assert_eq!(app.editor.drawing(), &before);
    for input in [
        "CIRCLE",
        "8,4",
        "0.75",
        "TEXT",
        "6,6",
        "0.5",
        "0",
        "Rust workflow",
        "DIM",
        "A",
        "0.25",
        "DIM",
        "6,2",
        "8,2",
        "8,1",
        "AB+12",
    ] {
        return_input(&mut app, input);
    }
    let drawing = app.editor.drawing().clone();
    assert_eq!(drawing.header.view, view);
    assert_eq!(drawing.header.limits, limits);
    assert!(drawing.entities().count() > before.entities().count());
    for extension in ["dwg", "DXF"] {
        let output = directory.0.join(format!("saved drawing.{extension}"));
        return_input(&mut app, "SAVE");
        return_input(&mut app, output.to_str().unwrap());
        assert_eq!(app.status, format!("Saved {}", output.display()));
        let bytes = std::fs::read(&output).unwrap();
        let reopened = decode_drawing(&bytes).unwrap();
        assert_eq!(reopened.header.view, view);
        assert_eq!(reopened.header.limits, limits);
        assert_eq!(reopened.header.snap, drawing.header.snap);
        assert_eq!(reopened.header.ortho, drawing.header.ortho);
        assert_eq!(reopened.header.dim_arrow, drawing.header.dim_arrow);
        assert_eq!(reopened.entities().count(), drawing.entities().count());
        assert_eq!(reopened.blocks().count(), drawing.blocks().count());
        if extension == "dwg" {
            assert_eq!(reopened.items, drawing.items);
        } else {
            // Historical DXF stores six decimal places; compare the entire
            // canonical serialized drawing rather than demanding exact f64s.
            assert_eq!(acad_dxf::write(&reopened), bytes);
            assert_eq!(bytes, acad_dxf::write(&drawing));
        }
        app.editor = acad_cmd::Editor::new(reopened);
        return_input(&mut app, "POINT");
        return_input(&mut app, "9,5");
        assert_eq!(
            app.editor.drawing().entities().count(),
            drawing.entities().count() + 1
        );
        return_input(&mut app, "UNDO");
        assert_eq!(
            app.editor.drawing().entities().count(),
            drawing.entities().count()
        );
    }
}

#[test]
fn failed_save_reports_error_and_editor_remains_usable() {
    let directory = WorkflowDirectory::new();
    let Some(mut app) = menu_app(0) else {
        return;
    };
    return_input(&mut app, "CIRCLE");
    return_input(&mut app, "4,5");
    return_input(&mut app, "2");
    let drawing = app.editor.drawing().clone();
    let output = directory.0.join("missing directory/drawing.dwg");
    return_input(&mut app, "SAVE");
    return_input(&mut app, output.to_str().unwrap());
    assert!(app.status.starts_with("Save failed:"));
    assert!(!output.exists());
    assert_eq!(app.editor.prompt(), "Command");
    assert_eq!(app.editor.drawing(), &drawing);
    return_input(&mut app, "POINT");
    return_input(&mut app, "1,2");
    assert_eq!(app.editor.drawing().entities().count(), 2);
    let output = directory.0.join("drawing.dwg");
    return_input(&mut app, "SAVE");
    return_input(&mut app, output.to_str().unwrap());
    assert_eq!(app.status, format!("Saved {}", output.display()));
    assert_eq!(
        decode_drawing(&std::fs::read(output).unwrap())
            .unwrap()
            .items,
        app.editor.drawing().items
    );
}

#[test]
fn startup_loads_acad_and_missing_menu_keeps_commands_usable() {
    let Some(mut app) = menu_app(2) else {
        return;
    };
    app.unload_menu();
    app.load_menu("ACAD");
    assert!(app.menu.is_some());
    assert_eq!(app.menu_page, 0);
    assert!(app.status.starts_with("Loaded 56 menu entries"));
    app.unload_menu();
    app.load_menu("/missing-startup-menu/ACAD.MNU");
    assert!(app.menu.is_none());
    assert!(app.status.starts_with("menu file not found:"));
    let mut pixels = vec![0; 640 * 480];
    command_line::draw(
        &mut pixels,
        640,
        480,
        app.editor.prompt(),
        &app.input,
        &app.status,
    );
    assert!(
        pixels.contains(&0x00ff_c080),
        "missing-file status is painted"
    );
    app.input = "LINE".into();
    app.submit_return_input(&mut |_, result| {
        result.unwrap();
    });
    assert!(app.editor.accepts_mouse_point());
    pixels.fill(0);
    command_line::draw(
        &mut pixels,
        640,
        480,
        app.editor.prompt(),
        &app.input,
        &app.status,
    );
    assert!(pixels.contains(&0x00ff_ffff));
}

#[test]
fn command_area_consumes_point_and_selection_clicks_with_or_without_menu() {
    for menu in [true, false] {
        for command in ["LINE", "ERASE"] {
            let Some(mut app) = menu_app(0) else {
                return;
            };
            if !menu {
                app.unload_menu();
            }
            app.editor.submit(command).unwrap();
            app.input = "unchanged".into();
            app.status = "unchanged status".into();
            let before = app.editor.drawing().clone();
            let prompt = app.editor.prompt().to_owned();
            for (x, y) in [(20.0, 436.0), (639.0, 479.0)] {
                app.cursor = Some((x, y));
                app.handle_left_click(640, 480, |_, _| panic!("command-area submission"));
                assert_eq!(app.input, "unchanged");
                assert_eq!(app.status, "unchanged status");
                assert_eq!(app.editor.prompt(), prompt);
                assert_eq!(app.editor.drawing(), &before);
            }
        }
    }
}

#[test]
fn drawing_overlays_stay_above_command_area_even_in_tiny_clients() {
    for height in [0, 1, 43, 44, 45, 46, 47, 48, 49, 100] {
        let canvas = command_line::drawing_height(height);
        let mut pixels = vec![0; 100 * height as usize];
        let vp = viewport_for(acad_cmd::Editor::default().drawing(), 100, height);
        draw_axis_ticks(&mut pixels, 100, canvas, &vp, 2.0);
        draw_crosshair(&mut pixels, 100, canvas, 20.0, canvas as f64);
        if canvas > 0 {
            draw_crosshair(&mut pixels, 100, canvas, 20.0, 0.0);
        }
        assert!(pixels[(100 * canvas as usize)..]
            .iter()
            .all(|&pixel| pixel == 0));
    }
}

#[test]
fn menu_loader_resolves_extension_and_preserves_macro_controls() {
    if crate::corpus_file("System/ACAD.MNU").is_none() {
        return;
    }
    let (path, menu) = load_menu_file("ACAD").unwrap();
    assert!(path.ends_with("corpus/System/ACAD.MNU"));
    assert!(menu
        .entries
        .iter()
        .any(|entry| entry.label == "^Snap" && entry.action == [0x02]));
    assert!(menu
        .entries
        .iter()
        .any(|entry| entry.label == "ZOOM All" && entry.action == b"zoom a"));
}

#[test]
fn crosshair_marks_both_axes_at_the_mouse_position() {
    let mut pixels = vec![0; 5 * 4];
    draw_crosshair(&mut pixels, 5, 4, 2.0, 1.0);

    for x in 0..5 {
        assert_eq!(pixels[5 + x], 0x00d060);
    }
    for y in 0..4 {
        assert_eq!(pixels[y * 5 + 2], 0x00d060);
    }
    assert_eq!(pixels[0], 0);
    assert_eq!(pixels[19], 0);
}

#[test]
fn crosshair_ignores_positions_outside_the_canvas() {
    let mut pixels = vec![0; 12];
    draw_crosshair(&mut pixels, 4, 3, -1.0, 1.0);
    draw_crosshair(&mut pixels, 4, 3, f64::NAN, 1.0);
    assert_eq!(pixels, vec![0; 12]);
}

#[test]
fn axis_setting_draws_ticks_at_world_space_multiples_on_each_edge() {
    let mut pixels = vec![0; 100 * 100];
    let viewport = Viewport::from_view(acad_model::Point { x: 0.0, y: 0.0 }, 10.0, 100, 100);
    draw_axis_ticks(&mut pixels, 100, 100, &viewport, 2.0);

    let tick = 0x0060_6060;
    assert_eq!(pixels[30], tick, "x=−2 reaches the top ruler");
    assert_eq!(pixels[99 * 100 + 70], tick, "x=2 reaches the bottom ruler");
    assert_eq!(pixels[30 * 100], tick, "y=2 reaches the left ruler");
    assert_eq!(pixels[70 * 100 + 99], tick, "y=−2 reaches the right ruler");
    assert_eq!(pixels[50 * 100 + 50], 0, "ticks do not cover the drawing");
}

#[test]
fn axis_tick_spacing_is_decimated_to_keep_dense_views_readable() {
    let mut pixels = vec![0; 100 * 100];
    let viewport = Viewport::from_view(acad_model::Point { x: 0.0, y: 0.0 }, 10.0, 100, 100);
    draw_axis_ticks(&mut pixels, 100, 100, &viewport, 0.01);
    let top_ticks = pixels[..100].iter().filter(|&&pixel| pixel != 0).count();
    assert!(
        (1..=20).contains(&top_ticks),
        "unexpected tick density {top_ticks}"
    );
}

#[test]
fn selection_highlight_uses_yellow_outlines_for_strokes_and_fills() {
    let line = vec![acad_model::Point { x: 1.0, y: 2.0 }];
    let polygon = vec![acad_model::Point { x: 3.0, y: 4.0 }];
    assert_eq!(
        highlight_primitives(vec![
            Prim::ColoredPolyline {
                points: line.clone(),
                rgb: [0, 255, 255],
            },
            Prim::FilledPolygon(polygon.clone()),
        ]),
        vec![
            Prim::ColoredPolyline {
                points: line,
                rgb: [255, 255, 0],
            },
            Prim::ColoredPolyline {
                points: polygon,
                rgb: [255, 255, 0],
            },
        ]
    );
}

#[test]
fn selection_highlight_uses_the_editor_ids_and_skips_load_records() {
    use acad_model::{DwgView, Entity, Extents, Header, Item, Mode, Point};
    let point = |x, y| Point { x, y };
    let line = |y| {
        Item::Entity(Entity::Line {
            start: point(0.0, y),
            end: point(2.0, y),
        })
    };
    let extents = Extents {
        xmin: -1.0,
        xmax: 3.0,
        ymin: -1.0,
        ymax: 6.0,
    };
    let drawing = acad_model::Drawing {
        header: Header {
            extents,
            limits: extents,
            base: point(0.0, 0.0),
            view: DwgView {
                center: point(0.0, 0.0),
                height: 10.0,
            },
            axis: Mode {
                on: false,
                spacing: 0.0,
            },
            snap: Mode {
                on: false,
                spacing: 0.0,
            },
            grid: Mode {
                on: false,
                spacing: 0.0,
            },
            ortho: false,
            fill: false,
            text_size: 1.0,
            trace_width: 0.0,
            fillet_radius: 0.0,
            dim_arrow: None,
            units: acad_model::Units {
                format: acad_model::UnitFormat::Decimal,
                precision: 4,
            },
            current_layer: 1,
            layers: Default::default(),
            off_layers: Default::default(),
            dwg_header_passthrough: None,
        },
        items: vec![
            line(1.0),
            Item::Entity(Entity::Load {
                name: "FONT".into(),
            }),
            line(4.0),
        ],
    };
    let viewport = Viewport::from_view(point(0.0, 0.0), 10.0, 100, 100);
    let selected = selected_highlight(&drawing, "2", &viewport, &Libraries::default());
    let expected_start = viewport.to_screen(point(0.0, 4.0));
    let expected_end = viewport.to_screen(point(2.0, 4.0));
    assert_eq!(
        selected,
        vec![Prim::ColoredPolyline {
            points: vec![expected_start, expected_end],
            rgb: [255, 255, 0],
        }]
    );
    let image = rasterize(&selected, 100, 100);
    assert!(image
        .pixels()
        .iter()
        .any(|pixel| pixel.red() > 100 && pixel.green() > 100 && pixel.blue() < 30));
}

#[test]
fn load_accepts_a_new_shp_path_and_makes_its_shapes_available() {
    let path = std::env::temp_dir().join(format!("acad-app-load-test-{}.SHP", std::process::id()));
    std::fs::write(&path, b"*129,13,STAR\n2,8,(4,2),1,9,(2,0),(0,2),(0,0),0;\n").unwrap();
    let mut app = Session {
        document: Document::unnamed(acad_cmd::Editor::default().drawing()),
        viewport_size: (800, 600),
        editor: acad_cmd::Editor::default(),
        libraries: Libraries::default(),
        menu: None,
        menu_page: 0,
        input: String::new(),
        status: String::new(),
        report: None,
        cursor: None,
        pending_insert_source: None,
        script: None,
        script_stepping: false,
        script_clock: std::sync::Arc::new(SystemClock::default()),
        paused_macro: None,
        directories: Vec::new(),
        main_menu: None,
        main_menu_home: false,
        main_menu_drawing: None,
        exit_requested: false,
        default_name_prompt: String::new(),
        sketch_drag: false,
    };
    app.editor.submit("LOAD").unwrap();
    let library_name = app.resolve_shape_library(path.to_str().unwrap()).unwrap();
    app.editor.submit(&library_name).unwrap();
    for input in ["SHAPE", "star", "1,2", "0.5", "45"] {
        app.editor.submit(input).unwrap();
    }
    let items = &app.editor.drawing().items;
    assert!(matches!(
        items.first(),
        Some(acad_model::Item::Entity(acad_model::Entity::OnLayer { entity, .. }))
            if matches!(entity.as_ref(), acad_model::Entity::Load { name } if name == &library_name)
    ));
    assert!(matches!(
        items.get(1),
        Some(acad_model::Item::Entity(acad_model::Entity::OnLayer { entity, .. }))
            if matches!(entity.as_ref(), acad_model::Entity::Shape { number: 129, .. })
    ));
    let viewport = viewport_for(app.editor.drawing(), 100, 100);
    let output = flatten_with_libraries(app.editor.drawing(), &viewport, &app.libraries);
    assert!(!output.primitives.is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn advance_menu_page_wraps_next() {
    let Some(bytes) = crate::corpus_file("System/ACAD.MNU") else {
        return;
    };
    let menu = acad_cmd::menu::parse_menu(&bytes).unwrap();
    assert_eq!(menu_panel::page_count(&menu), 3);
    assert_eq!(advance_menu_page(0, &menu), 1);
    assert_eq!(advance_menu_page(1, &menu), 2);
    // NEXT on the last page wraps back to page 0.
    assert_eq!(advance_menu_page(2, &menu), 0);
}

/// A session with the retained ACAD.MNU loaded, or `None` (a visible skip)
/// without the corpus.
fn menu_app(page: usize) -> Option<Session> {
    crate::corpus_file("System/ACAD.MNU")?;
    Some(Session {
        document: Document::unnamed(acad_cmd::Editor::default().drawing()),
        viewport_size: (800, 600),
        editor: acad_cmd::Editor::default(),
        libraries: Libraries::default(),
        menu: Some(load_menu_file("ACAD").unwrap().1),
        menu_page: page,
        input: String::new(),
        status: String::new(),
        report: None,
        cursor: None,
        pending_insert_source: None,
        script: None,
        script_stepping: false,
        script_clock: std::sync::Arc::new(SystemClock::default()),
        paused_macro: None,
        directories: Vec::new(),
        main_menu: None,
        main_menu_home: false,
        main_menu_drawing: None,
        exit_requested: false,
        default_name_prompt: String::new(),
        sketch_drag: false,
    })
}

fn click(app: &mut Session, width: u32, height: u32) {
    app.handle_left_click(width, height, |app, result| {
        assert!(matches!(result.unwrap(), acad_cmd::Effect::Continue));
        app.status = app.editor.status().to_owned();
    });
}

#[test]
fn app_mouse_route_consumes_every_blank_slot_during_point_and_selection_prompts() {
    let (width, height) = (640, 480);
    for (page, blank_rows) in [(1, vec![0, 1]), (2, vec![0])] {
        for row in blank_rows {
            let Some(mut app) = menu_app(page) else {
                return;
            };
            let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), page, width, height);
            app.cursor = Some((
                (layout.rect.0 + 2) as f64,
                (row * layout.row_height + 2) as f64,
            ));
            app.editor.submit("POINT").unwrap();
            app.input = "pending input".into();
            app.status = "unchanged".into();
            click(&mut app, width, height);
            assert!(app.editor.accepts_mouse_point());
            assert_eq!(app.editor.drawing().entities().count(), 0);
            assert_eq!(app.input, "pending input");
            assert_eq!(app.status, "unchanged");

            // Put an entity under the blank panel slot. A leaked click
            // would now select it, rather than merely finding no entity.
            let (x, y) = app.cursor.unwrap();
            let world = viewport_for(app.editor.drawing(), width, height)
                .to_world(acad_model::Point { x, y });
            let original_header = app.editor.drawing().header.clone();
            app.editor.submit_mouse_point(world).unwrap();
            let mut drawing = app.editor.drawing().clone();
            drawing.header = original_header;
            app.editor = acad_cmd::Editor::new(drawing);
            assert_eq!(app.editor.pick_entity_at(world, 0.00001), Some(1));
            app.editor.submit("ERASE").unwrap();
            assert!(app.editor.accepts_mouse_selection());
            app.input.clear();
            click(&mut app, width, height);
            assert!(app.input.is_empty(), "blank row selected a hidden entity");
            assert_eq!(app.status, "unchanged");
        }
    }
}

#[test]
fn app_mouse_route_preserves_outside_panel_point_conversion_and_selection() {
    let (width, height) = (640, 480);
    let Some(mut app) = menu_app(1) else {
        return;
    };
    app.cursor = Some((100.0, 200.0));
    let expected = viewport_for(app.editor.drawing(), width, height)
        .to_world(acad_model::Point { x: 100.0, y: 200.0 });
    app.editor.submit("POINT").unwrap();
    app.input = "pending input".into();
    click(&mut app, width, height);
    assert!(app.input.is_empty());
    assert_eq!(app.editor.drawing().entities().count(), 1);
    // Prove the submitted point used the drawing's coordinate conversion.
    // First-entity creation changes the view, so pick its new screen position.
    assert_eq!(app.editor.pick_entity_at(expected, 0.00001), Some(1));
    let screen = viewport_for(app.editor.drawing(), width, height).to_screen(expected);
    app.cursor = Some((screen.x, screen.y));
    app.editor.submit("ERASE").unwrap();
    assert!(app.editor.accepts_mouse_selection());
    click(&mut app, width, height);
    assert_eq!(app.input, "1");
    assert_eq!(app.status, "Selected entity 1");
}

#[test]
fn snapped_crosshair_matches_the_submitted_point_across_client_sizes() {
    for (width, height) in [(640, 480), (1280, 960), (960, 540)] {
        let Some(mut app) = menu_app(0) else {
            return;
        };
        for input in ["SNAP", "0.5", "POINT"] {
            app.editor.submit(input).unwrap();
        }
        let vp = viewport_for(app.editor.drawing(), width, height);
        let raw = vp.to_screen(acad_model::Point { x: 2.24, y: 3.26 });
        app.cursor = Some((raw.x, raw.y));
        let expected = acad_model::Point { x: 2.0, y: 3.5 };
        let screen = vp.to_screen(expected);
        let before = app.editor.drawing().clone();
        let preview = app.crosshair_position(width, height).unwrap();
        assert!((preview.0 - screen.x).abs() < 1e-10);
        assert!((preview.1 - screen.y).abs() < 1e-10);
        assert_eq!(app.editor.drawing(), &before);
        click(&mut app, width, height);
        assert_eq!(app.editor.pick_entity_at(expected, 1e-10), Some(1));
    }
}

#[test]
fn ortho_menu_toggle_changes_the_pending_crosshair_and_click_together() {
    let Some(mut app) = menu_app(0) else {
        return;
    };
    for input in ["SNAP", "1", "LINE", "1.2,2.3"] {
        app.editor.submit(input).unwrap();
    }
    control_cursor(&mut app, 0x0f);
    assert_eq!(app.crosshair_position(640, 480), None);
    click(&mut app, 640, 480);
    assert_eq!(app.editor.prompt(), "LINE: next point (Enter to finish)");
    let vp = viewport_for(app.editor.drawing(), 640, 480);
    let raw = vp.to_screen(acad_model::Point { x: 4.6, y: 3.49 });
    app.cursor = Some((raw.x, raw.y));
    let preview = app.crosshair_position(640, 480).unwrap();
    let expected = vp.to_screen(acad_model::Point { x: 5.0, y: 2.3 });
    assert!((preview.0 - expected.x).abs() < 1e-10);
    assert!((preview.1 - expected.y).abs() < 1e-10);
    click(&mut app, 640, 480);
    let entity = app.editor.drawing().entities().next().unwrap();
    let acad_model::Entity::OnLayer { entity, .. } = entity else {
        panic!("layered LINE")
    };
    assert!(matches!(entity.as_ref(),acad_model::Entity::Line{start,end}
            if *start == acad_model::Point{x:1.2,y:2.3} && *end == acad_model::Point{x:5.0,y:2.3}));
}

#[test]
fn mouse_selection_remains_at_the_raw_cursor_with_snap_and_ortho_on() {
    let Some(mut app) = menu_app(0) else {
        return;
    };
    for input in ["POINT", "2.24,3.26", "SNAP", "10", "ORTHO", "ON", "ERASE"] {
        app.editor.submit(input).unwrap();
    }
    let screen = viewport_for(app.editor.drawing(), 640, 480)
        .to_screen(acad_model::Point { x: 2.24, y: 3.26 });
    app.cursor = Some((screen.x, screen.y));
    assert_eq!(app.crosshair_position(640, 480), app.cursor);
    click(&mut app, 640, 480);
    assert_eq!(app.input, "1");
    assert_eq!(app.status, "Selected entity 1");
}

#[test]
fn crosshair_suppresses_interface_and_invalid_positions_without_mutation() {
    let Some(mut app) = menu_app(0) else {
        return;
    };
    for input in ["SNAP", "1", "POINT"] {
        app.editor.submit(input).unwrap();
    }
    let before = app.editor.drawing().clone();
    control_cursor(&mut app, 0x02);
    assert_eq!(app.crosshair_position(640, 480), None);
    for cursor in [(20.0, 450.0), (-1.0, 20.0), (640.0, 20.0), (20.0, f64::NAN)] {
        app.cursor = Some(cursor);
        assert_eq!(app.crosshair_position(640, 480), None);
    }
    assert_eq!(app.editor.drawing(), &before);
    assert_eq!(app.editor.prompt(), "POINT: point");
}

#[test]
fn app_mouse_route_dispatches_point_macro() {
    let (width, height) = (640, 480);
    let Some(mut app) = menu_app(1) else {
        return;
    };
    let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), 1, width, height);
    app.cursor = Some((
        (layout.rect.0 + 2) as f64,
        (2 * layout.row_height + 2) as f64,
    ));
    click(&mut app, width, height);
    assert!(
        app.editor.accepts_mouse_point(),
        "POINT row must submit its macro"
    );
}

#[test]
fn loaded_menu_minimum_keeps_next_rendered_and_routable_at_supported_sizes() {
    use winit::dpi::PhysicalSize;
    let Some(mut app) = menu_app(0) else {
        return;
    };
    let menu = app.menu.as_ref().unwrap();
    let minimum = menu_min_inner_size(Some(menu)).unwrap();
    assert_eq!(minimum, PhysicalSize::new(160, 336 + command_line::HEIGHT));
    for requested_height in [335, 336, 337, 379, 380, 381] {
        let requested = PhysicalSize::new(640, requested_height);
        let supported = supported_client_size(Some(menu), requested);
        assert_eq!(
            supported.height,
            requested_height.max(336 + command_line::HEIGHT)
        );
        assert_eq!(supported_client_size(None, requested), requested);
        for page in 0..3 {
            let canvas = command_line::drawing_height(supported.height);
            let layout = menu_panel::layout_for(menu, page, supported.width, canvas);
            assert_eq!(
                layout.rect.3,
                (minimum.height - command_line::HEIGHT) as usize
            );
            let next_y = (layout.rect.3 - layout.row_height + 2) as f64;
            assert_eq!(
                menu_panel::entry_at(menu, page, &layout, (layout.rect.0 + 2) as f64, next_y),
                Some(menu_panel::PanelHit::Next)
            );
            let mut buffer = vec![0; supported.width as usize * supported.height as usize];
            menu_panel::draw_panel(
                &mut buffer[..supported.width as usize * canvas as usize],
                supported.width,
                canvas,
                menu,
                page,
                &layout,
            );
            command_line::draw(
                &mut buffer,
                supported.width,
                supported.height,
                "Command",
                "",
                "Ready",
            );
            assert!(buffer[supported.width as usize * canvas as usize..].contains(&0x00ff_ffff));
            let text_x = layout.rect.0 + crate::bitmap::GLYPH_WIDTH;
            assert!(
                ((layout.rect.3 - layout.row_height)..layout.rect.3).any(|y| {
                    (text_x..text_x + 16)
                        .any(|x| buffer[y * supported.width as usize + x] == 0x00ff_ffff)
                })
            );
        }
    }
    assert_eq!(
        menu_min_inner_size(None),
        Some(PhysicalSize::new(160, 1 + command_line::HEIGHT))
    );
    assert_eq!(
        supported_client_size(Some(menu), PhysicalSize::new(159, 335)),
        minimum
    );
    // At the smallest supported physical client size, actual Session routing
    // still cycles all recovered pages through the visible NEXT slot.
    for expected_page in [1, 2, 0] {
        app.cursor = Some((2.0, 322.0));
        click(&mut app, minimum.width, minimum.height);
        assert_eq!(app.menu_page, expected_page);
    }
}

#[test]
fn app_mouse_route_has_no_panel_without_a_loaded_menu() {
    let Some(mut app) = menu_app(1) else {
        return;
    };
    app.menu = None;
    app.cursor = Some((482.0, 2.0));
    app.editor.submit("POINT").unwrap();
    click(&mut app, 640, 480);
    assert_eq!(app.editor.drawing().entities().count(), 1);
}

fn menu_control_fixture(path: &str) -> acad_model::Drawing {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/recovery/2026-10-01-menu-controls")
        .join(path);
    acad_dwg::parse(&std::fs::read(path).unwrap()).unwrap()
}

fn control_cursor(app: &mut Session, byte: u8) {
    let menu = app.menu.as_ref().unwrap();
    let layout = menu_panel::layout_for(menu, app.menu_page, 640, 480);
    app.cursor = (0..21).find_map(|row| {
        let (x, y) = (
            (layout.rect.0 + 2) as f64,
            (row * layout.row_height + 2) as f64,
        );
        let menu_panel::PanelHit::Entry(index) =
            menu_panel::entry_at(menu, app.menu_page, &layout, x, y)?
        else {
            return None;
        };
        (menu_panel::resolve_entry(menu, app.menu_page, index)?.action == [byte]).then_some((x, y))
    });
    assert!(app.cursor.is_some(), "control byte {byte:x} missing");
}

#[test]
fn app_mouse_route_menu_controls_preserve_prompt_and_consume_panel() {
    for (byte, status) in [(0x02, "<Snap on>"), (0x0f, "<Ortho on>")] {
        for setup in [vec!["LINE", "2.1,3.15"], vec!["CIRCLE", "2,3"]] {
            let Some(mut app) = menu_app(0) else {
                return;
            };
            app.editor.drawing_mut().header.snap.spacing = 0.5;
            for input in setup {
                app.editor.submit(input).unwrap();
            }
            let prompt = app.editor.prompt().to_owned();
            let mut expected = app.editor.drawing().clone();
            if byte == 0x02 {
                expected.header.snap.on = true;
            } else {
                expected.header.ortho = true;
            }
            app.input = "pending text".into();
            control_cursor(&mut app, byte);
            let mut callbacks = 0;
            app.handle_left_click(640, 480, |app, result| {
                callbacks += 1;
                assert_eq!(result, Ok(acad_cmd::Effect::Continue));
                app.status = app.editor.status().to_owned();
            });
            assert_eq!(callbacks, 1);
            assert_eq!(app.status, status);
            assert_eq!(app.input, "pending text");
            assert_eq!(app.editor.prompt(), prompt);
            assert_eq!(app.editor.drawing(), &expected);
            assert_eq!(app.menu_page, 0);
            assert!(app.menu.is_some());
            let line = prompt.starts_with("LINE");
            for input in if line {
                vec!["4.25,5.15", ""]
            } else {
                vec!["1.25"]
            } {
                app.editor.submit(input).unwrap();
            }
            let id = match (byte, line) {
                (0x02, true) => "FSLINE",
                (0x0f, true) => "FOLINE",
                (0x02, false) => "SCIRCLE",
                (0x0f, false) => "OCIRCLE",
                _ => unreachable!(),
            };
            let native = menu_control_fixture(&format!("controls/drawings/{id}.dwg"));
            assert_eq!(app.editor.drawing().items, native.items);
            assert_eq!(app.editor.drawing().header.snap, native.header.snap);
            assert_eq!(app.editor.drawing().header.ortho, native.header.ortho);
        }
    }
    let Some(mut app) = menu_app(0) else {
        return;
    };
    let entry = app
        .menu
        .as_mut()
        .unwrap()
        .entries
        .iter_mut()
        .find(|entry| entry.action == [0x02])
        .unwrap();
    entry.action = vec![0x04];
    app.editor.submit("POINT").unwrap();
    app.input = "unchanged".into();
    control_cursor(&mut app, 0x04);
    app.handle_left_click(640, 480, |_, _| panic!("unknown control callback"));
    assert!(app.status.contains("0x04"));
    assert_eq!(app.input, "unchanged");
    assert!(app.editor.accepts_mouse_point());
    assert!(app.editor.drawing().items.is_empty());
    for page in [1, 2] {
        app.menu_page = page;
        let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), page, 640, 480);
        app.cursor = Some(((layout.rect.0 + 2) as f64, 2.0));
        app.handle_left_click(640, 480, |_, _| panic!("blank callback"));
        assert!(app.editor.drawing().items.is_empty());
        assert_eq!(app.menu_page, page);
    }
}

#[test]
fn app_mouse_route_menu_cancel_clears_buffers_and_retains_selection() {
    for (setup, buffer) in [
        (vec![], "p"),
        (vec!["LINE"], "2"),
        (vec!["ERASE"], "1"),
        (vec!["MENU"], "AC"),
    ] {
        let Some(mut app) = menu_app(0) else {
            return;
        };
        for input in setup {
            app.editor.submit(input).unwrap();
        }
        app.input = buffer.into();
        let original = app.editor.drawing().clone();
        control_cursor(&mut app, 0x03);
        let mut callbacks = 0;
        app.handle_left_click(640, 480, |app, result| {
            callbacks += 1;
            assert_eq!(result, Ok(acad_cmd::Effect::Continue));
            app.status = app.editor.status().to_owned();
        });
        assert_eq!(callbacks, 1);
        assert!(app.input.is_empty());
        assert_eq!(app.editor.prompt(), "Command");
        assert_eq!(app.status, "*Cancel*");
        assert_eq!(app.editor.drawing(), &original);
        assert_eq!(app.menu_page, 0);
        assert!(app.menu.is_some());
        for input in ["POINT", "8,7"] {
            app.editor.submit(input).unwrap();
        }
        assert_eq!(app.editor.drawing().items.len(), 1);
        assert_eq!(
            app.editor
                .pick_entity_at(acad_model::Point { x: 8.0, y: 7.0 }, 1e-12),
            Some(1)
        );
        let id = match buffer {
            "p" => "CBIDLE",
            "2" => "CBPOINT",
            "1" => "CBSELECT",
            _ => continue,
        };
        let native = menu_control_fixture(&format!("cancel-tail/drawings/{id}.dwg"));
        assert_eq!(app.editor.drawing().items, native.items);
        assert_eq!(app.editor.drawing().header.snap, native.header.snap);
        assert_eq!(app.editor.drawing().header.ortho, native.header.ortho);
    }
    let Some(mut app) = menu_app(0) else {
        return;
    };
    app.editor = acad_cmd::Editor::new(menu_control_fixture("pilot/drawings/CSELECT.dwg"));
    let original = app.editor.drawing().clone();
    app.editor.submit("ERASE").unwrap();
    let screen = viewport_for(app.editor.drawing(), 640, 480).to_screen(acad_model::Point {
        x: 6.0,
        y: 3.58823529411769,
    });
    app.cursor = Some((screen.x, screen.y));
    click(&mut app, 640, 480);
    assert_eq!(app.input, "1");
    control_cursor(&mut app, 0x03);
    click(&mut app, 640, 480);
    assert!(app.input.is_empty());
    assert_eq!(app.status, "*Cancel*");
    assert_eq!(app.editor.prompt(), "Command");
    assert_eq!(app.editor.drawing(), &original);
}

#[test]
fn app_mouse_route_cancel_preserves_later_pages() {
    for (page, id) in [(1, "CPAGE1"), (2, "CPAGE2")] {
        let Some(mut app) = menu_app(page) else {
            return;
        };
        let menu = app.menu.clone();
        app.editor.submit("LINE").unwrap();
        app.input = "2".into();
        let original = app.editor.drawing().clone();
        control_cursor(&mut app, 0x03);
        let mut callbacks = 0;
        app.handle_left_click(640, 480, |app, result| {
            callbacks += 1;
            assert_eq!(result, Ok(acad_cmd::Effect::Continue));
            app.status = app.editor.status().to_owned();
        });
        assert_eq!(callbacks, 1);
        assert_eq!(app.menu_page, page);
        assert_eq!(app.menu, menu);
        assert!(app.menu.is_some());
        assert!(app.input.is_empty());
        assert_eq!(app.editor.prompt(), "Command");
        assert_eq!(app.status, "*Cancel*");
        assert_eq!(app.editor.drawing(), &original);
        for input in ["POINT", "8,7"] {
            app.editor.submit(input).unwrap();
        }
        let native = menu_control_fixture(&format!("cancel-tail/drawings/{id}.dwg"));
        assert_eq!(app.editor.drawing().items, native.items);
        assert_eq!(app.editor.drawing().header.snap, native.header.snap);
        assert_eq!(app.editor.drawing().header.ortho, native.header.ortho);
    }
}
fn go_cursor(app: &mut Session) {
    let menu = app.menu.as_ref().unwrap();
    assert!(menu
        .entries
        .iter()
        .any(|entry| entry.kind == acad_cmd::menu::MenuEntryKind::Header && entry.action == b";"));
    let layout = menu_panel::layout_for(menu, app.menu_page, 640, 480);
    app.cursor = Some(((layout.rect.0 + 2) as f64, 2.0));
    assert_eq!(
        menu_panel::entry_at(menu, app.menu_page, &layout, app.cursor.unwrap().0, 2.0),
        Some(menu_panel::PanelHit::Go)
    );
}

fn return_result(app: &mut Session, result: Result<acad_cmd::Effect, String>) {
    match result.unwrap() {
        acad_cmd::Effect::Continue => app.status = app.editor.status().into(),
        acad_cmd::Effect::LoadMenu(name) => {
            app.menu = Some(load_menu_file(&name).unwrap().1);
            app.menu_page = 0;
        }
        acad_cmd::Effect::UnloadMenu => app.unload_menu(),
        effect => panic!("unexpected effect: {effect:?}"),
    }
}

fn return_or_go(app: &mut Session, go: bool) -> usize {
    let mut count = 0;
    let mut callback = |app: &mut Session, result| {
        count += 1;
        return_result(app, result);
    };
    if go {
        go_cursor(app);
        app.handle_left_click(640, 480, &mut callback);
    } else {
        app.submit_return_input(&mut callback);
    }
    count
}

#[test]
fn app_mouse_route_go_and_return_repeat_menu_once_then_unload() {
    for go in [false, true] {
        let Some(mut app) = menu_app(0) else {
            return;
        };
        for input in ["MENU", "ACAD"] {
            app.input = input.into();
            assert_eq!(return_or_go(&mut app, false), 1);
        }
        assert_eq!(return_or_go(&mut app, go), 1);
        assert_eq!(app.editor.prompt(), "File name");
        assert!(app.menu.is_some());
        assert_eq!(app.menu_page, 0);
        assert_eq!(return_or_go(&mut app, go), 1);
        assert_eq!(app.editor.prompt(), "Command");
        assert!(app.menu.is_none());
        assert_eq!(app.menu_page, 0);
        assert_eq!(app.status, "");
        assert!(app.input.is_empty());
        assert!(app.editor.drawing().items.is_empty());
        assert_eq!(
            menu_min_inner_size(app.menu.as_ref()).unwrap().height,
            1 + command_line::HEIGHT
        );
    }
}

#[test]
fn app_mouse_route_menu_cancel_keeps_panel() {
    let Some(mut app) = menu_app(0) else {
        return;
    };
    app.editor.submit_return("MENU").unwrap();
    app.input = "pending".into();
    control_cursor(&mut app, 0x03);
    let mut count = 0;
    app.handle_left_click(640, 480, |app, result| {
        count += 1;
        assert_eq!(result, Ok(acad_cmd::Effect::Continue));
        return_result(app, result);
    });
    assert_eq!(count, 1);
    assert_eq!(app.status, "*Cancel*");
    assert_eq!(app.editor.prompt(), "Command");
    assert!(app.menu.is_some());
    assert_eq!(app.menu_page, 0);
    assert!(app.input.is_empty());
}

#[test]
fn app_mouse_route_go_empty_prompts_and_history() {
    for (setup, status, id) in [
        (vec!["LINE"], "*Invalid*", "GOFIRST"),
        (vec!["LINE", "2,3"], "", "GONEXT"),
        (vec!["CIRCLE", "2,3"], "*Invalid*", "GORADIUS"),
        (vec!["LINE", "2,3", "4,5", "", "ERASE"], "", "GOSELECT"),
    ] {
        let Some(mut app) = menu_app(0) else {
            return;
        };
        for input in setup {
            app.editor.submit_return(input).unwrap();
        }
        assert_eq!(return_or_go(&mut app, true), 1);
        assert_eq!(app.status, status);
        assert_eq!(app.editor.prompt(), "Command");
        assert!(app.input.is_empty());
        assert_eq!(app.menu_page, 0);
        let native = menu_control_fixture(&format!("go/drawings/{id}.dwg"));
        assert_eq!(app.editor.drawing().items, native.items);
    }
    let Some(mut app) = menu_app(0) else {
        return;
    };
    go_cursor(&mut app);
    app.handle_left_click(640, 480, |app, result| {
        assert_eq!(
            result,
            Err("Unknown command. Type ? for list of commands.".into())
        );
        app.status = result.unwrap_err();
    });
    assert_eq!(app.status, app.editor.status());
    app.input = ";".into();
    assert_eq!(app.input, ";");
    app.submit_return_input(&mut |app, result| {
        assert_eq!(
            result,
            Err("Unknown command. Type ? for list of commands.".into())
        );
        app.status = result.unwrap_err();
    });
    assert!(app.input.is_empty());
    for input in ["POINT", "8,7"] {
        app.editor.submit_return(input).unwrap();
    }
    let items = app.editor.drawing().items.clone();
    for go in [false, true] {
        assert_eq!(return_or_go(&mut app, go), 1);
        assert_eq!(app.editor.prompt(), "POINT: point");
        assert_eq!(app.editor.drawing().items, items);
        app.editor
            .apply_menu_control(acad_cmd::MenuControl::Cancel)
            .unwrap();
    }
}

#[test]
fn app_mouse_route_go_submits_pending_point_and_selection_once() {
    let Some(mut app) = menu_app(0) else {
        return;
    };
    app.editor.submit_return("LINE").unwrap();
    app.input = "2,3".into();
    assert_eq!(return_or_go(&mut app, true), 1);
    assert!(app.input.is_empty());
    assert_eq!(app.editor.prompt(), "LINE: next point (Enter to finish)");
    for input in ["4,5", ""] {
        app.input = input.into();
        assert_eq!(return_or_go(&mut app, true), 1);
    }
    assert_eq!(
        app.editor.drawing().items,
        menu_control_fixture("buffers/drawings/GOBUFPNT.dwg").items
    );
    for go in [false, true] {
        let Some(mut app) = menu_app(0) else {
            return;
        };
        let mut seed = menu_control_fixture("pilot/drawings/CSELECT.dwg");
        seed.items.truncate(1); // Exclude CSELECT's later continuation POINT.
        app.editor = acad_cmd::Editor::new(seed);
        let original = app.editor.drawing().items.clone();
        app.editor.submit_return("ERASE").unwrap();
        let screen = viewport_for(app.editor.drawing(), 640, 480).to_screen(acad_model::Point {
            x: 6.0,
            y: 3.58823529411769,
        });
        app.cursor = Some((screen.x, screen.y));
        click(&mut app, 640, 480);
        assert_eq!(app.input, "1");
        assert_eq!(return_or_go(&mut app, go), 1);
        assert!(app.input.is_empty());
        assert_eq!(app.status, "1 selected, 1 found.");
        assert_eq!(app.editor.prompt(), "Command");
        assert_eq!(
            app.editor.drawing().items,
            original
                .into_iter()
                .map(|item| match item {
                    acad_model::Item::Entity(entity) => acad_model::Item::Erased(entity),
                    _ => panic!("seed must be a LINE"),
                })
                .collect::<Vec<_>>()
        );
        if go {
            for input in ["POINT", "8,7"] {
                app.editor.submit_return(input).unwrap();
            }
            assert_eq!(
                app.editor.drawing().items,
                menu_control_fixture("buffers/drawings/GOBUFSEL.dwg").items
            );
        } else {
            assert_eq!(
                app.editor.drawing().items,
                menu_control_fixture("buffers/drawings/CSELBASE.dwg").items
            );
        }
    }
}

#[test]
fn app_mouse_route_return_and_go_preserve_load_resolution() {
    for go in [false, true] {
        let Some(mut app) = menu_app(0) else {
            return;
        };
        let path =
            std::env::temp_dir().join(format!("acad-go-load-{}-{go}.SHP", std::process::id()));
        std::fs::write(&path, b"*129,13,STAR\n2,8,(4,2),1,9,(2,0),(0,2),(0,0),0;\n").unwrap();
        app.editor.submit_return("LOAD").unwrap();
        app.input = path.to_str().unwrap().into();
        assert_eq!(return_or_go(&mut app, go), 1);
        assert!(app.input.is_empty());
        assert_eq!(app.editor.prompt(), "Command");
        let name = path.file_stem().unwrap().to_str().unwrap();
        assert!(app.libraries.get(name).is_some());
        for input in ["SHAPE", "star", "1,2", "0.5", "45"] {
            app.editor.submit_return(input).unwrap();
        }
        assert_eq!(app.editor.drawing().items.len(), 2);
        assert!(!flatten_with_libraries(
            app.editor.drawing(),
            &viewport_for(app.editor.drawing(), 640, 480),
            &app.libraries
        )
        .primitives
        .is_empty());
        std::fs::remove_file(path).unwrap();
        for (input, error) in [
            (
                "menu-controls-no-such-library",
                "shape library is not available: menu-controls-no-such-library",
            ),
            ("", "shape library name cannot be empty"),
        ] {
            app.editor.submit_return("LOAD").unwrap();
            app.input = input.into();
            assert_eq!(return_or_go(&mut app, go), 0);
            assert!(app.input.is_empty());
            assert_eq!(app.status, error);
            assert!(app.editor.awaiting_shape_library_name());
            app.editor.cancel_command().unwrap();
        }
    }
    // Plain macro pieces retain their LOAD bypass, even when the SHP exists
    // on disk and physical Return could resolve it. The trailing `;` is the
    // path's own Return; item end adds none (menu_macros.rs).
    let Some(mut app) = menu_app(0) else {
        return;
    };
    let path = std::env::temp_dir().join(format!("acad-macro-load-{}.SHP", std::process::id()));
    std::fs::write(&path, b"*129,13,STAR\n2,8,(4,2),1,9,(2,0),(0,2),(0,0),0;\n").unwrap();
    app.menu = Some(
        acad_cmd::menu::parse_menu(format!("[Load]LOAD {};\n", path.display()).as_bytes()).unwrap(),
    );
    let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), 0, 640, 480);
    app.cursor = Some(((layout.rect.0 + 2) as f64, (layout.row_height + 2) as f64));
    app.input = "kept".into();
    let mut results = Vec::new();
    app.handle_left_click(640, 480, |_, result| results.push(result));
    assert_eq!(
        results,
        vec![
            Ok(acad_cmd::Effect::Continue),
            Err(format!(
                "shape library is not available: {}",
                path.display()
            )),
        ]
    );
    assert_eq!(app.input, "kept");
    assert!(app.editor.awaiting_shape_library_name());
    assert!(app.editor.drawing().items.is_empty());
    assert!(app
        .libraries
        .get(path.file_stem().unwrap().to_str().unwrap())
        .is_none());
    app.editor.cancel_command().unwrap();
    assert_eq!(
        app.editor.submit_return(""),
        Err("Unknown command. Type ? for list of commands.".into())
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn app_mouse_route_go_header_only_and_next_still_wraps() {
    let Some(mut app) = menu_app(0) else {
        return;
    };
    go_cursor(&mut app);
    for page in [1, 2] {
        app.menu_page = page;
        app.editor.submit("LINE").unwrap();
        app.input = "2,3".into();
        let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), page, 640, 480);
        app.cursor = Some(((layout.rect.0 + 2) as f64, 2.0));
        let mut count = 0;
        app.handle_left_click(640, 480, |_, _| count += 1);
        assert_eq!(count, 0);
        assert_eq!(app.menu_page, page);
        assert_eq!(app.input, "2,3");
        assert_eq!(app.editor.prompt(), "LINE: first point");
        assert!(app.editor.drawing().items.is_empty());
        app.editor.cancel_command().unwrap();
    }
    let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), 2, 640, 480);
    app.cursor = Some((
        (layout.rect.0 + 2) as f64,
        (layout.rect.3 - layout.row_height + 2) as f64,
    ));
    app.handle_left_click(640, 480, |_, _| panic!("NEXT submitted input"));
    assert_eq!(app.menu_page, 0);
    assert_eq!(app.input, "2,3");
}

#[test]
fn selection_repeat_highlight_and_api_count_share_whole_group_ids() {
    use acad_model::{Entity, Item, Point, Repeat};
    let p = |x, y| Point { x, y };
    let mut editor = acad_cmd::Editor::default();
    editor.drawing_mut().items = vec![
        Item::Entity(Entity::Load {
            name: "FONT".into(),
        }),
        Item::Repeat(Repeat {
            start_layer: 1,
            end_layer: 1,
            entities: vec![
                Entity::Line {
                    start: p(0.0, 0.0),
                    end: p(2.0, 0.0),
                },
                Entity::Point {
                    origin: p(1.0, 1.0),
                },
            ],
            columns: 2,
            rows: 2,
            column_spacing: 4.0,
            row_spacing: 3.0,
        }),
        Item::Entity(Entity::Line {
            start: p(20.0, 20.0),
            end: p(21.0, 20.0),
        }),
    ];
    let viewport = Viewport::from_view(p(0.0, 0.0), 10.0, 100, 100);
    let highlight = selected_highlight(editor.drawing(), "1", &viewport, &Libraries::default());
    assert_eq!(
        highlight.len(),
        12,
        "line plus the point cross at all four cells"
    );
    assert!(highlight.contains(&Prim::ColoredPolyline {
        points: vec![
            viewport.to_screen(p(4.0, 3.0)),
            viewport.to_screen(p(6.0, 3.0))
        ],
        rgb: [255, 255, 0]
    }));
    assert_eq!(editor.pick_entity_at(p(5.0, 3.0), 0.0), Some(1));
    assert_eq!(editor.pick_entity_at(p(20.5, 20.0), 0.0), Some(2));
    let mut session = Session::with_editor(editor, Libraries::default());
    assert_eq!(crate::api::state(&session)["entities"], 4);
    assert_eq!(crate::api::state(&session)["selectable_objects"], 2);
    session.command("LIST").unwrap();
    session.command("ALL").unwrap();
    assert!(session
        .report_text()
        .unwrap()
        .starts_with("1 REPEAT, 2 LINE\n"));
}

#[test]
fn unusable_stored_view_displays_limits_then_unit_fallback_without_mutation() {
    let point = |x, y| acad_model::Point { x, y };
    let mut drawing = acad_cmd::Editor::default().drawing().clone();
    drawing.header.view = acad_model::DwgView {
        center: point(1e308, 1e308),
        height: 1.0,
    };
    let canvas = command_line::drawing_height(244);
    let limits = drawing.header.limits;
    let vp = viewport_for(&drawing, 400, 244);
    // LIMITS fit: their center is the canvas center and they are fully shown.
    let center = point(
        limits.xmin / 2.0 + limits.xmax / 2.0,
        limits.ymin / 2.0 + limits.ymax / 2.0,
    );
    assert_eq!(vp.to_screen(center), point(200.0, f64::from(canvas) / 2.0));
    let corner = vp.to_screen(point(limits.xmax, limits.ymax));
    assert!(corner.x <= 400.0 && corner.y >= 0.0);
    // Unusable LIMITS too: a unit view at the origin.
    drawing.header.limits = acad_model::Extents {
        xmin: 1.0,
        ymin: 1.0,
        xmax: 1.0,
        ymax: 1.0,
    };
    let before = drawing.clone();
    let vp = viewport_for(&drawing, 400, 244);
    assert_eq!(
        vp.to_screen(point(0.0, 0.0)),
        point(200.0, f64::from(canvas) / 2.0)
    );
    assert_eq!(vp.to_world(point(200.0, 0.0)), point(0.0, 0.5));
    assert_eq!(drawing, before);
}

#[test]
fn menu_macro_insert_pieces_resolve_drawing_files_like_return() {
    // Macro pieces split on whitespace, so the path must not contain any.
    let path = std::env::temp_dir().join(format!("MACROPART{}.DWG", std::process::id()));
    let mut source = acad_cmd::Editor::default().drawing().clone();
    source
        .items
        .push(acad_model::Item::Entity(acad_model::Entity::Point {
            origin: acad_model::Point { x: 1.0, y: 1.0 },
        }));
    std::fs::write(&path, acad_dwg::write(&source).unwrap()).unwrap();
    let Some(mut app) = menu_app(0) else {
        return;
    };
    app.menu = Some(
        acad_cmd::menu::parse_menu(
            format!("[Part]INSERT {} 2,2 1 1 0;\n", path.display()).as_bytes(),
        )
        .unwrap(),
    );
    let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), 0, 640, 480);
    app.cursor = Some(((layout.rect.0 + 2) as f64, (layout.row_height + 2) as f64));
    let mut results = Vec::new();
    app.handle_left_click(640, 480, |_, result| results.push(result));
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let name = format!("MACROPART{}", std::process::id());
    assert!(app.editor.drawing().block(&name).is_some());
    assert_eq!(app.editor.prompt(), "Command");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn menu_macro_hatch_pieces_read_pattern_files_like_return() {
    // Macro pieces split on whitespace, so the path must not contain any.
    let path = std::env::temp_dir().join(format!("MACROPAT{}.PAT", std::process::id()));
    std::fs::write(
        &path,
        format!("*MACROPAT{}\n0, 0,0, 0,.5\n", std::process::id()),
    )
    .unwrap();
    let Some(mut app) = menu_app(0) else {
        return;
    };
    for input in ["LINE", "0,0", "2,0", "2,2", "0,2", "C"] {
        app.editor.submit(input).unwrap();
    }
    app.menu = Some(
        acad_cmd::menu::parse_menu(
            format!("[Hatch]HATCH {},O 1 0 ALL;\n", path.display()).as_bytes(),
        )
        .unwrap(),
    );
    let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), 0, 640, 480);
    app.cursor = Some(((layout.rect.0 + 2) as f64, (layout.row_height + 2) as f64));
    let mut results = Vec::new();
    app.handle_left_click(640, 480, |_, result| results.push(result));
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(app.editor.drawing().blocks().count(), 1);
    assert_eq!(app.editor.prompt(), "Command");
    std::fs::remove_file(path).unwrap();
}

/// Load `menu` and pick its item on panel `row` (row 0 is the GO/blank slot).
fn pick_macro(app: &mut Session, menu: &[u8], row: usize) -> Vec<Result<acad_cmd::Effect, String>> {
    app.menu = Some(acad_cmd::menu::parse_menu(menu).unwrap());
    app.menu_page = 0;
    let layout = menu_panel::layout_for(app.menu.as_ref().unwrap(), 0, 640, 480);
    app.cursor = Some((
        (layout.rect.0 + 2) as f64,
        (row * layout.row_height + 2) as f64,
    ));
    let mut results = Vec::new();
    app.handle_left_click(640, 480, |app, result| {
        results.push(result.clone());
        let _ = app.apply_effect(result);
    });
    results
}

fn bare_lines(app: &Session) -> Vec<(f64, f64, f64, f64)> {
    app.drawing()
        .entities()
        .filter_map(|mut entity| {
            while let acad_model::Entity::OnLayer { entity: inner, .. } = entity {
                entity = inner;
            }
            match entity {
                acad_model::Entity::Line { start, end } => Some((start.x, start.y, end.x, end.y)),
                _ => None,
            }
        })
        .collect()
}

#[test]
fn macro_backslash_pauses_for_one_typed_or_pointed_input_then_continues() {
    // crates/acad-oracle/tests/menu_macros.rs measures each case.
    let Some(mut app) = menu_app(0) else {
        return;
    };
    pick_macro(&mut app, b"[A]line 1,1 \\;\r\n", 1);
    assert_eq!(app.prompt(), app.editor.prompt());
    assert!(app.paused_macro.is_some());
    assert!(!app.command("7,7").unwrap());
    assert_eq!(bare_lines(&app), [(1.0, 1.0, 7.0, 7.0)]);
    assert_eq!(app.prompt(), "Command");
    assert!(app.paused_macro.is_none());

    let Some(mut app) = menu_app(0) else {
        return;
    };
    pick_macro(&mut app, b"[A]line \\\\;\r\n", 1);
    app.point(acad_model::Point { x: 1.0, y: 1.0 }).unwrap();
    assert!(app.paused_macro.is_some());
    app.point(acad_model::Point { x: 2.0, y: 2.0 }).unwrap();
    assert_eq!(bare_lines(&app), [(1.0, 1.0, 2.0, 2.0)]);
    assert_eq!(app.prompt(), "Command");

    // Text before the pause is the start of the user's input.
    let Some(mut app) = menu_app(0) else {
        return;
    };
    pick_macro(&mut app, b"[A]line 1,\\7,7;\r\n", 1);
    assert_eq!(app.input(), "1,");
    app.type_characters("1").unwrap();
    let typed = app.input().to_owned();
    app.command(&typed).unwrap();
    assert_eq!(bare_lines(&app), [(1.0, 1.0, 7.0, 7.0)]);

    // A trailing pause adds no Return: LINE stays open after the input.
    let Some(mut app) = menu_app(0) else {
        return;
    };
    pick_macro(&mut app, b"[A]line 1,1 \\\r\n", 1);
    app.command("2,2").unwrap();
    assert!(app.paused_macro.is_none());
    app.command("3,3").unwrap();
    assert_eq!(
        bare_lines(&app),
        [(1.0, 1.0, 2.0, 2.0), (2.0, 2.0, 3.0, 3.0)]
    );
}

#[test]
fn macro_spaces_are_returns_and_item_end_adds_no_extra_return() {
    let Some(mut app) = menu_app(0) else {
        return;
    };
    let results = pick_macro(&mut app, b"[A]line 2,2  3,3;\r\n", 1);
    assert_eq!(results.len(), 4);
    assert!(bare_lines(&app).is_empty());
    for menu in [&b"[A]line 2,2 3,3\r\n"[..], b"[A]line 2,2 3,3;\r\n"] {
        let Some(mut app) = menu_app(0) else {
            return;
        };
        pick_macro(&mut app, menu, 1);
        app.command("4,4").unwrap();
        assert_eq!(
            bare_lines(&app),
            [(2.0, 2.0, 3.0, 3.0), (3.0, 3.0, 4.0, 4.0)]
        );
    }
}

#[test]
fn cancel_new_pick_and_reset_abandon_a_paused_macro() {
    let menu = b"[A]line 1,1 \\9,9;\r\n[B]line 4,4 5,5;\r\n";
    let Some(mut app) = menu_app(0) else {
        return;
    };
    pick_macro(&mut app, menu, 1);
    app.cancel().unwrap();
    assert!(app.paused_macro.is_none());
    for input in ["LINE", "5,5", "6,6", ""] {
        app.command(input).unwrap();
    }
    assert_eq!(bare_lines(&app), [(5.0, 5.0, 6.0, 6.0)]);

    let Some(mut app) = menu_app(0) else {
        return;
    };
    pick_macro(&mut app, menu, 1);
    app.cancel().unwrap();
    pick_macro(&mut app, menu, 2);
    assert!(app.paused_macro.is_none());
    assert_eq!(bare_lines(&app), [(4.0, 4.0, 5.0, 5.0)]);

    let Some(mut app) = menu_app(0) else {
        return;
    };
    pick_macro(&mut app, menu, 1);
    app.reset();
    assert!(app.paused_macro.is_none());
}

#[test]
fn retained_subdiv_erase_macro_pauses_for_selection_and_blank_slots_are_inert() {
    let Some(menu) = crate::corpus_file("Samples/SUBDIV.MNU") else {
        return;
    };
    let parsed = acad_cmd::menu::parse_menu(&menu).unwrap();
    let Some(mut app) = menu_app(0) else {
        return;
    };
    for input in ["LINE", "0,0", "1,1", ""] {
        app.command(input).unwrap();
    }
    app.menu = Some(parsed);
    // `[ERASE 1]erase \;` is the fourth entry of SUBDIV.MNU's second page.
    let (page, index) = (1, 3);
    let entry = menu_panel::resolve_entry(app.menu.as_ref().unwrap(), page, index).unwrap();
    assert_eq!(entry.label, "ERASE 1");
    let text = String::from_utf8(entry.action.clone()).unwrap();
    app.run_macro(&text, &mut |app, result| {
        let _ = app.apply_effect(result);
    });
    assert!(app.paused_macro.is_some());
    app.command("L").unwrap();
    assert!(bare_lines(&app).is_empty());
    assert_eq!(app.prompt(), "Command");

    // A blank slot submits nothing, even at an open prompt.
    let Some(mut app) = menu_app(0) else {
        return;
    };
    app.command("LINE").unwrap();
    let results = pick_macro(&mut app, b"[A]x\r\n\r\n[B]y\r\n", 2);
    assert!(results.is_empty());
    assert_eq!(app.prompt(), app.editor.prompt());
    app.command("1,1").unwrap();
    app.command("2,2").unwrap();
    assert_eq!(bare_lines(&app), [(1.0, 1.0, 2.0, 2.0)]);
}

#[test]
fn menu_files_are_regular_byte_capped_reads() {
    let root = std::env::temp_dir().join(format!("acad-menu-cap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir(&root).unwrap();
    let big = root.join("BIG.MNU");
    std::fs::write(&big, vec![b'A'; MAX_MENU_FILE_BYTES as usize + 1]).unwrap();
    let error = load_menu_file(big.to_str().unwrap()).unwrap_err();
    assert!(
        error.contains("exceeds the 65536-byte menu file limit"),
        "{error}"
    );
    // A directory never shadows a later regular candidate.
    std::fs::create_dir(root.join("DIR")).unwrap();
    std::fs::write(root.join("DIR.MNU"), b"LINE\r\n").unwrap();
    let (path, menu) = load_menu_file(root.join("DIR").to_str().unwrap()).unwrap();
    assert_eq!(path, root.join("DIR.MNU"));
    assert_eq!(menu.entries[0].label, "LINE");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn api_command_at_a_pause_appends_to_the_typed_prefix_like_gui_typing() {
    let Some(mut app) = menu_app(0) else {
        return;
    };
    pick_macro(&mut app, b"[A]line 1,\\7,7;\r\n", 1);
    assert_eq!(app.input(), "1,");
    crate::api::dispatch(
        &mut app,
        serde_json::from_value(serde_json::json!({"method":"command","params":{"input":"1"}}))
            .unwrap(),
        (640, 480),
    )
    .unwrap();
    assert_eq!(bare_lines(&app), [(1.0, 1.0, 7.0, 7.0)]);
    // Without a pause, API command still replaces the input line.
    app.cancel().unwrap();
    app.set_input("ignored".into());
    for input in ["LINE", "2,2", "3,3", ""] {
        crate::api::dispatch(
            &mut app,
            serde_json::from_value(
                serde_json::json!({"method":"command","params":{"input":input}}),
            )
            .unwrap(),
            (640, 480),
        )
        .unwrap();
    }
    assert_eq!(bare_lines(&app).len(), 2);
}

#[test]
fn a_directory_sync_failure_after_the_rename_is_a_successful_save_with_a_warning() {
    let root = std::env::temp_dir().join(format!("acad-dirsync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir(&root).unwrap();
    let path = root.join("D.DWG");
    let mut app = Session::default();
    app.save(&path).unwrap();
    let previous = std::fs::read(&path).unwrap();
    let mut app = Session::open(&path, &[]).unwrap();
    app.command("POINT").unwrap();
    app.command("1,1").unwrap();
    crate::document::FAIL_DIRECTORY_SYNC.with(|fail| fail.set(Some(std::io::ErrorKind::Other)));
    let saved = app.save(&path);
    crate::document::FAIL_DIRECTORY_SYNC.with(|fail| fail.set(None));
    saved.unwrap();
    assert!(!app.is_dirty());
    assert_eq!(app.document_path(), Some(path.as_path()));
    assert!(
        app.status()
            .contains("; directory sync not confirmed: injected"),
        "{}",
        app.status()
    );
    assert_eq!(std::fs::read(root.join("D.BAK")).unwrap(), previous);
    assert_eq!(
        Session::open(&path, &[]).unwrap().drawing().items,
        app.drawing().items
    );
    // END with the same failure still completes (and exits).
    app.command("POINT").unwrap();
    app.command("2,2").unwrap();
    crate::document::FAIL_DIRECTORY_SYNC.with(|fail| fail.set(Some(std::io::ErrorKind::Other)));
    let ended = app.command("END");
    crate::document::FAIL_DIRECTORY_SYNC.with(|fail| fail.set(None));
    assert!(ended.unwrap());
    assert!(!app.is_dirty());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_pattern_file_answer_at_a_macro_pause_resumes_the_macro() {
    let Some(mut app) = menu_app(0) else {
        return;
    };
    let path = std::env::temp_dir().join(format!("PAUSEPAT{}.PAT", std::process::id()));
    std::fs::write(
        &path,
        format!("*PAUSEPAT{}\n0, 0,0, 0,.5\n", std::process::id()),
    )
    .unwrap();
    for input in ["LINE", "0,0", "2,0", "2,2", "0,2", "C"] {
        app.editor.submit(input).unwrap();
    }
    pick_macro(&mut app, b"[A]hatch \\1;0;all;\r\n", 1);
    assert!(app.paused_macro.is_some());
    app.command(&path.display().to_string()).unwrap();
    assert!(app.paused_macro.is_none());
    assert_eq!(app.editor.drawing().blocks().count(), 1);
    assert_eq!(app.editor.prompt(), "Command");
    std::fs::remove_file(path).unwrap();
}

/// Menu macros reach the WBLOCK replace question like typed input: a macro
/// written for a new file (`wblock \*`) has its `*` decline at the question,
/// and only an explicit `Y` piece confirms (docs/native-files-menu.md).
#[test]
fn menu_macro_wblock_answers_the_replace_question_with_its_next_piece() {
    let dir = std::env::temp_dir().join(format!("acad-macro-wblock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let target = dir.join("part.dwg");
    std::fs::write(&target, b"keep me").unwrap();
    let mut app = Session::default();
    app.command("POINT").unwrap();
    app.command("1,1").unwrap();
    let run = |app: &mut Session, text: &str| {
        app.run_macro(text, &mut |app, result| {
            let _ = app.apply_effect(result);
        });
        assert!(app.paused_macro.is_some());
        app.command_typed(target.to_str().unwrap()).unwrap();
        assert!(app.paused_macro.is_none());
        assert_eq!(app.prompt(), "Command");
    };
    run(&mut app, "wblock \\*");
    assert_eq!(std::fs::read(&target).unwrap(), b"keep me");
    run(&mut app, "wblock \\y *");
    let written = std::fs::read(&target).unwrap();
    assert_eq!(acad_dwg::parse(&written).unwrap().entities().count(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unnamed_end_asks_the_original_erasure_question_after_the_path() {
    let directory = WorkflowDirectory::new();
    let mut drawing = acad_dxf::parse(
        b"REPEAT,1\r\nLINE,1\r\n0,0,0,4\r\nLINE,1\r\n1,0,5,0\r\nENDREP,1\r\n1,1,0,0\r\n",
    )
    .unwrap();
    let acad_model::Item::Repeat(r) = &mut drawing.items[0] else {
        panic!("group")
    };
    r.entities[0] = acad_model::Entity::Erased(Box::new(r.entities[0].clone()));
    let mut session = Session::with_editor(acad_cmd::Editor::new(drawing), Libraries::default());
    session.command("ERASE").unwrap();
    session.command("1").unwrap();
    let erased = session.drawing().clone();
    let path = directory.0.join("FRESH.DWG");
    assert!(!session.command("END").unwrap());
    assert_eq!(session.prompt(), "END: output file");
    assert!(!session.command(path.to_str().unwrap()).unwrap());
    assert!(session
        .prompt()
        .starts_with("END: Lose earlier member erasure"));
    assert!(!path.exists());
    assert!(session.command("YES").unwrap());
    let (expected, count) = acad_model::group_codec::original_member_erasure(&erased);
    assert_eq!(count, 1);
    assert_eq!(
        acad_dwg::parse(&std::fs::read(&path).unwrap())
            .unwrap()
            .items,
        expected.items
    );
}

/// A macro has no answer to the native-only member-erasure question: a
/// non-`Y` piece declines and drops the rest of the macro; `y` confirms.
#[test]
fn menu_macro_declining_the_native_erasure_question_drops_its_remaining_steps() {
    let dir = std::env::temp_dir().join(format!("acad-macro-erasure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let out = dir.join("out.dwg");
    let target = out.to_str().unwrap();
    let start = || {
        let mut drawing = acad_dxf::parse(
            b"REPEAT,1\r\nLINE,1\r\n0,0,0,4\r\nLINE,1\r\n1,0,5,0\r\nENDREP,1\r\n1,1,0,0\r\n",
        )
        .unwrap();
        let acad_model::Item::Repeat(r) = &mut drawing.items[0] else {
            panic!("group")
        };
        r.entities[0] = acad_model::Entity::Erased(Box::new(r.entities[0].clone()));
        Session::with_editor(acad_cmd::Editor::new(drawing), Libraries::default())
    };
    let run = |app: &mut Session, text: &str| {
        app.run_macro(text, &mut |app, result| {
            let _ = app.apply_effect(result);
        });
    };
    let mut app = start();
    run(&mut app, &format!("erase 1 save {target} redraw point 1,1"));
    assert!(!out.exists());
    assert!(app.status().contains("nothing written"), "{}", app.status());
    assert_eq!(app.prompt(), "Command");
    assert!(app.paused_macro.is_none());
    assert_eq!(app.drawing().entities().count(), 0);
    assert!(app.is_dirty());
    let mut app = start();
    run(&mut app, &format!("erase 1 save {target} y point 1,1"));
    let written = acad_dwg::parse(&std::fs::read(&out).unwrap()).unwrap();
    assert!(matches!(&written.items[0], acad_model::Item::Repeat(r)
        if r.entities.iter().all(acad_model::Entity::is_erased)));
    assert_eq!(app.drawing().entities().count(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_member_erasure_question_fits_an_800_pixel_command_line_and_explains_on_status() {
    let mut drawing = acad_dxf::parse(
        b"REPEAT,1\r\nLINE,1\r\n0,0,0,4\r\nLINE,1\r\n1,0,5,0\r\nENDREP,1\r\n1,1,0,0\r\n",
    )
    .unwrap();
    let acad_model::Item::Repeat(r) = &mut drawing.items[0] else {
        panic!("group")
    };
    r.entities[0] = acad_model::Entity::Erased(Box::new(r.entities[0].clone()));
    let mut app = Session::with_editor(acad_cmd::Editor::new(drawing), Libraries::default());
    app.command("ERASE").unwrap();
    app.command("1").unwrap();
    let cells = (800 - 8) / 16;
    for (command, path) in [("SAVE", Some("never-written.dwg")), ("END", None)] {
        app.command(command).unwrap();
        if let Some(path) = path {
            app.command(path).unwrap();
        } else {
            assert_eq!(app.prompt(), "END: output file");
            app.command("never-written.dwg").unwrap();
        }
        assert!(app.prompt().starts_with(command));
        assert!(format!("{}: _", app.prompt()).chars().count() <= cells);
        assert!(app
            .status()
            .starts_with("Erased REPEAT group holds earlier erased members"));
        assert!(app.status().contains("AutoCAD 1.4"));
        app.cancel().unwrap();
    }
    assert!(!std::path::Path::new("never-written.dwg").exists());
}
