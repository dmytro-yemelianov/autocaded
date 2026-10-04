//! RB2: one whole-frame aggregate budget across every owner (see
//! docs/native-render-budget.md). Pathological drawings are built in-test.
use acad_model::{Block, Drawing, Entity, Item, Point, Repeat};
use acad_render::{
    flatten_selected_with_budget, flatten_with_budget, flatten_with_libraries, rasterize,
    FrameBudget, Libraries, Prim, Viewport, FRAME_WORK_LIMIT, PRIMITIVE_SETUP_UNITS,
};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

fn empty_drawing() -> Drawing {
    let mut drawing = acad_dxf::parse(b"POINT,1\r\n1,1\r\n").unwrap();
    drawing.items.clear();
    drawing
}

/// 250 x 199 cells + one child stays just inside the per-owner preflight
/// (100,000 generated records), so only the aggregate budget can stop it.
fn point_lattice() -> Repeat {
    lattice(250, 199)
}

fn lattice(columns: u16, rows: u16) -> Repeat {
    Repeat {
        start_layer: 1,
        end_layer: 1,
        entities: vec![Entity::Point {
            origin: Point { x: 1.0, y: 1.0 },
        }],
        columns,
        rows,
        column_spacing: 0.04,
        row_spacing: 0.04,
    }
}

fn units(prims: &[Prim]) -> usize {
    prims
        .iter()
        .map(|prim| match prim {
            Prim::Polyline(p) | Prim::FilledPolygon(p) => p.len(),
            Prim::ColoredPolyline { points, .. } | Prim::ColoredFilledPolygon { points, .. } => {
                points.len()
            }
        })
        .map(|points| points + PRIMITIVE_SETUP_UNITS)
        .sum()
}

/// GUI-responsiveness regression: 65,535 owners each just under the
/// per-owner budget would be ~6.5e9 generated records (hours of work, tens of
/// GB of primitives) before RB2. The frame now stops at a deterministic owner
/// boundary, bounded in time and memory, and says so.
#[test]
fn many_owners_each_under_the_owner_budget_stop_at_a_deterministic_boundary() {
    let mut drawing = empty_drawing();
    drawing.items = vec![Item::Repeat(point_lattice()); usize::from(u16::MAX)];
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let started = Instant::now();
    let mut budget = FrameBudget::default();
    let output = flatten_with_budget(&drawing, &vp, &Libraries::default(), &mut budget);
    let flatten = started.elapsed();
    let raster_started = Instant::now();
    let _pixels = rasterize(&output.primitives, 800, 552);
    let raster = raster_started.elapsed();
    eprintln!(
        "RB2 pathological frame (debug={}): flatten {flatten:?}, rasterize {raster:?}, \
         {} primitives, {} work units",
        cfg!(debug_assertions),
        output.primitives.len(),
        budget.used()
    );

    let stop = output.budget_stop.expect("budget stop");
    // Each owner: 49,750 cells x (one POINT cross, 2 strokes x (2 + 4) units,
    // + 1 per cell) = 646,750 units, so exactly one whole owner fits.
    assert_eq!(stop.first_skipped_item, 2);
    assert_eq!(stop.skipped_owners, usize::from(u16::MAX) - 1);
    assert_eq!(stop.limit, FRAME_WORK_LIMIT);
    assert_eq!(output.primitives.len(), 49_750 * 2, "one whole owner");
    assert!(budget.exhausted() && budget.used() <= FRAME_WORK_LIMIT);
    assert!(units(&output.primitives) <= FRAME_WORK_LIMIT);
    assert_eq!(
        output.diagnostics,
        vec![format!(
            "Frame render budget of {FRAME_WORK_LIMIT} work units exceeded: drawing stopped \
             before item 2; {} owner(s) not drawn",
            usize::from(u16::MAX) - 1
        )]
    );
    // Stated bound (docs/native-render-budget.md): a budget-stopped frame
    // completes in seconds even in an unoptimized build under test load.
    assert!(
        flatten + raster < Duration::from_secs(30),
        "{flatten:?} + {raster:?}"
    );
}

#[test]
fn thousands_of_inserts_of_a_heavy_block_are_bounded_by_the_frame() {
    let mut drawing = empty_drawing();
    let circles = (0..1_000)
        .map(|i| Entity::Circle {
            center: Point {
                x: f64::from(i % 40),
                y: f64::from(i / 40),
            },
            radius: 0.4,
        })
        .collect();
    drawing.items.push(Item::Block(Block {
        name: "HEAVY".into(),
        base: Point { x: 0.0, y: 0.0 },
        entities: circles,
    }));
    for i in 0..5_000 {
        drawing.items.push(Item::Entity(Entity::Insert {
            name: "HEAVY".into(),
            origin: Point {
                x: f64::from(i),
                y: 0.0,
            },
            x_scale: 1.0,
            y_scale: 1.0,
            rotation_deg: 0.0,
        }));
    }
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let output = flatten_with_libraries(&drawing, &vp, &Libraries::default());
    let stop = output.budget_stop.expect("budget stop");
    // 1,000 circles x (91 + 4) units, charged at generation and transform.
    assert_eq!(stop.first_skipped_item, 7, "five whole inserts drawn");
    assert_eq!(stop.skipped_owners, 4_995);
    assert_eq!(output.primitives.len(), 5 * 1_000);
    assert!(output.diagnostics[0].contains("Frame render budget"));
}

#[test]
fn shp_instructions_count_even_when_text_draws_no_strokes() {
    let mut libraries = Libraries::default();
    // Shape A: pen up, move, end. Three instructions and no stroke per glyph.
    libraries
        .insert("TXT", b"*0,4,Font\n6,1,0,0\n*65,3,A\n2,021,0")
        .unwrap();
    let mut drawing = empty_drawing();
    let text = Entity::Text {
        origin: Point { x: 1.0, y: 1.0 },
        height: 1.0,
        rotation_deg: 0.0,
        value: "A".repeat(10_000),
    };
    drawing.items = vec![Item::Entity(text); 200];
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let output = flatten_with_libraries(&drawing, &vp, &libraries);
    assert!(output.primitives.is_empty());
    let stop = output.budget_stop.expect("budget stop");
    // 30,000 instructions + 2 visits per text: 33 texts fit in 1,000,000.
    assert_eq!(stop.first_skipped_item, 34);
    assert_eq!(stop.skipped_owners, 167);
}

#[test]
fn owners_within_budget_are_complete_and_report_no_stop() {
    let mut drawing = empty_drawing();
    drawing.items = vec![
        Item::Repeat(point_lattice()),
        Item::Repeat(lattice(100, 100)),
    ];
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let mut budget = FrameBudget::default();
    let output = flatten_with_budget(&drawing, &vp, &Libraries::default(), &mut budget);
    assert!(output.budget_stop.is_none());
    assert!(output.diagnostics.is_empty());
    assert_eq!(output.primitives.len(), (49_750 + 10_000) * 2);
    assert!(!budget.exhausted());
    assert!(budget.used() >= units(&output.primitives));
}

#[test]
fn selection_highlight_shares_the_frame_budget_and_is_never_partial() {
    let mut drawing = empty_drawing();
    // 10,000 cells x 13 units = 130,000 per owner; nine owners exceed.
    drawing.items = vec![Item::Repeat(lattice(100, 100)); 9];
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let libraries = Libraries::default();

    // The highlight alone fits a fresh budget.
    let selected = BTreeSet::from([0, 8]);
    let mut fresh = FrameBudget::default();
    let alone = flatten_selected_with_budget(&drawing, &vp, &libraries, &selected, &mut fresh);
    assert!(alone.budget_stop.is_none() && alone.diagnostics.is_empty());
    assert_eq!(alone.primitives.len(), 2 * 10_000 * 2);

    // After the drawing pass spent the frame, the same highlight is omitted
    // entirely rather than showing only the first selected owner.
    let mut budget = FrameBudget::default();
    let drawn = flatten_with_budget(&drawing, &vp, &libraries, &mut budget);
    assert!(drawn.budget_stop.is_some());
    let highlight = flatten_selected_with_budget(&drawing, &vp, &libraries, &selected, &mut budget);
    assert!(highlight.primitives.is_empty());
    assert!(highlight.budget_stop.is_some());
    assert_eq!(
        highlight.diagnostics,
        vec![format!(
            "Selection highlight exceeds frame render budget of {FRAME_WORK_LIMIT} work units; \
             highlight omitted"
        )]
    );
}

#[test]
fn hidden_owners_and_preflight_visits_are_charged() {
    let mut drawing = empty_drawing();
    drawing.header.off_layers.insert(5);
    let hidden = Entity::OnLayer {
        layer: 5,
        entity: Box::new(Entity::Point {
            origin: Point { x: 1.0, y: 1.0 },
        }),
    };
    drawing.items = vec![Item::Entity(hidden); 10_000];
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let mut budget = FrameBudget::new(5_000);
    let output = flatten_with_budget(&drawing, &vp, &Libraries::default(), &mut budget);
    assert!(output.primitives.is_empty());
    let stop = output.budget_stop.expect("hidden work is not free");
    assert!(stop.first_skipped_item > 1 && stop.first_skipped_item < 10_000);
    assert_eq!(stop.skipped_owners, 10_000 - (stop.first_skipped_item - 1));
}

#[test]
fn budget_stop_is_deterministic() {
    let mut drawing = empty_drawing();
    drawing.items = vec![Item::Repeat(point_lattice()); 6];
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let a = flatten_with_libraries(&drawing, &vp, &Libraries::default());
    let b = flatten_with_libraries(&drawing, &vp, &Libraries::default());
    assert_eq!(a.primitives, b.primitives);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert_eq!(a.budget_stop, b.budget_stop);
}

fn timed<T>(label: &str, run: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = run();
    let elapsed = started.elapsed();
    eprintln!(
        "RB2 {label} (debug={}): {elapsed:?}",
        cfg!(debug_assertions)
    );
    assert!(elapsed < Duration::from_secs(30), "{label}: {elapsed:?}");
    value
}

/// Review P1: a lattice whose base renders nothing (no members, or only
/// erased members) still costs one unit per cell and never loops for free.
#[test]
fn empty_and_erased_only_lattices_are_charged_per_cell() {
    let erased = Entity::Erased(Box::new(Entity::Point {
        origin: Point { x: 1.0, y: 1.0 },
    }));
    for members in [vec![], vec![erased]] {
        let mut drawing = empty_drawing();
        let mut repeat = lattice(316, 316);
        repeat.entities = members;
        drawing.items = vec![Item::Repeat(repeat); usize::from(u16::MAX)];
        let vp = Viewport::fit(&drawing.header.limits, 800, 552);
        let mut budget = FrameBudget::default();
        let output = timed("empty-base lattices", || {
            flatten_with_budget(&drawing, &vp, &Libraries::default(), &mut budget)
        });
        assert!(output.primitives.is_empty());
        let stop = output.budget_stop.expect("cells are not free");
        // 99,856 cells (+ visits) per owner: ten owners fit.
        assert_eq!(stop.first_skipped_item, 11);
        assert!(output.incomplete);
    }
}

/// Review P1: SHP programs that fail are charged for what they executed, and
/// per-pass diagnostics are capped, hashed and quote only a TEXT prefix.
#[test]
fn failing_glyphs_are_charged_and_diagnostics_stay_bounded() {
    let mut libraries = Libraries::default();
    libraries
        .insert("TXT", b"*0,4,Font\n6,1,0,0\n*65,3,A\n2,021,0")
        .unwrap();
    let mut drawing = empty_drawing();
    drawing.items = (0..8_000)
        .map(|i| {
            Item::Entity(Entity::Text {
                origin: Point { x: 1.0, y: 1.0 },
                height: 1.0,
                rotation_deg: 0.0,
                // 40,000 glyphs x 3 instructions exceeds the SHP budget.
                value: format!("{}{i}", "A".repeat(40_000)),
            })
        })
        .collect();
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let output = timed("failing TEXT owners", || {
        flatten_with_libraries(&drawing, &vp, &libraries)
    });
    let stop = output.budget_stop.expect("failed SHP work is charged");
    // 100,000 instructions + 1 visit + preflight per text: nine fit.
    assert_eq!(stop.first_skipped_item, 10);
    assert!(
        output.diagnostics.len() <= 66,
        "{}",
        output.diagnostics.len()
    );
    assert!(output.diagnostics.iter().all(|d| d.len() < 200));
    assert!(output.diagnostics[0].contains("SHP instruction budget exceeded"));
    assert!(output
        .diagnostics
        .last()
        .unwrap()
        .starts_with("Frame render budget"));
}

#[test]
fn many_rejected_owners_report_a_capped_diagnostic_list() {
    let mut drawing = empty_drawing();
    // Zero columns: every owner is refused by its per-owner preflight.
    drawing.items = vec![Item::Repeat(lattice(0, 1)); usize::from(u16::MAX)];
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let output = timed("rejected owners", || {
        flatten_with_libraries(&drawing, &vp, &Libraries::default())
    });
    assert!(output.budget_stop.is_none());
    assert!(output.incomplete, "skipped owners are not a complete frame");
    assert_eq!(output.diagnostics.len(), 65);
    assert_eq!(output.diagnostics[0], "Owner 1 exceeds render work budget");
    assert_eq!(
        output.diagnostics[64],
        format!(
            "... and {} more render diagnostic(s) not listed",
            usize::from(u16::MAX) - 64
        )
    );
}

/// Review P2: a LOAD/recursion context failure stops the owner loop without a
/// budget stop; the pass is still reported incomplete.
#[test]
fn context_failure_marks_the_pass_incomplete() {
    let mut drawing = acad_dxf::parse(
        b"BLOCK,1\r\n0,0\r\nLOOP\r\nINSERT,1\r\n0,0,1,1,0\r\nLOOP\r\nENDBLK,1\r\nLINE,1\r\n0,0,1,1\r\nINSERT,2\r\n0,0,1,1,0\r\nLOOP\r\nLINE,1\r\n2,2,3,3\r\n",
    )
    .unwrap();
    drawing.header.off_layers.insert(2);
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let output = flatten_with_libraries(&drawing, &vp, &Libraries::default());
    assert!(output.budget_stop.is_none());
    assert_eq!(output.primitives.len(), 1, "only the first LINE");
    assert!(output.incomplete);
    let complete = flatten_with_libraries(&empty_drawing(), &vp, &Libraries::default());
    assert!(!complete.incomplete);
}

fn insert(name: &str) -> Entity {
    Entity::Insert {
        name: name.into(),
        origin: Point { x: 1.0, y: 1.0 },
        x_scale: 1.0,
        y_scale: 1.0,
        rotation_deg: 0.0,
    }
}

/// Review pass 2 (h2): INSERT resolution must not rescan every drawing item.
/// 190 owners x 2,000 nested INSERTs of a block defined after 65,335 filler
/// records took 87 s in release with a linear `Drawing::block`.
#[test]
fn block_lookups_use_a_per_pass_index() {
    let mut drawing = empty_drawing();
    drawing.items.push(Item::Block(Block {
        name: "A".into(),
        base: Point { x: 0.0, y: 0.0 },
        entities: vec![insert("Z"); 2_000],
    }));
    drawing.items.extend(std::iter::repeat_n(
        Item::Erased(Entity::Point {
            origin: Point { x: 0.0, y: 0.0 },
        }),
        65_335,
    ));
    drawing.items.push(Item::Block(Block {
        name: "Z".into(),
        base: Point { x: 0.0, y: 0.0 },
        entities: vec![],
    }));
    drawing
        .items
        .extend(std::iter::repeat_n(Item::Entity(insert("A")), 190));
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let mut budget = FrameBudget::default();
    let output = timed("indexed block lookups", || {
        flatten_with_budget(&drawing, &vp, &Libraries::default(), &mut budget)
    });
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(!output.incomplete);
}

/// Review pass 2 (h3): repeated LOADs of one shape library keep a single
/// stack entry, so each SHAPE scans at most the supplied libraries.
#[test]
fn repeated_shape_loads_keep_a_bounded_stack() {
    let mut libraries = Libraries::default();
    libraries.insert("S", b"*201,2,X\n021,0").unwrap();
    let mut drawing = empty_drawing();
    let shape = Entity::Shape {
        origin: Point { x: 1.0, y: 1.0 },
        height: 1.0,
        rotation_deg: 0.0,
        number: 200,
    };
    drawing.items.extend(std::iter::repeat_n(
        Item::Entity(Entity::Load { name: "S".into() }),
        32_767,
    ));
    drawing
        .items
        .extend(std::iter::repeat_n(Item::Entity(shape), 32_767));
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let output = timed("shape stack", || {
        flatten_with_libraries(&drawing, &vp, &libraries)
    });
    assert_eq!(
        output.diagnostics,
        vec!["SHAPE 200: no loaded library defines this shape".to_string()]
    );
}

#[test]
fn shape_search_order_is_unchanged_by_stack_dedup() {
    let mut libraries = Libraries::default();
    // Both define shape 201; S draws +x, T draws +y.
    libraries.insert("S", b"*201,2,X\n010,0").unwrap();
    libraries.insert("T", b"*201,2,Y\n014,0").unwrap();
    let source = |loads: &[&str]| {
        let mut drawing = empty_drawing();
        for name in loads {
            drawing.items.push(Item::Entity(Entity::Load {
                name: (*name).into(),
            }));
        }
        drawing.items.push(Item::Entity(Entity::Shape {
            origin: Point { x: 1.0, y: 1.0 },
            height: 1.0,
            rotation_deg: 0.0,
            number: 201,
        }));
        drawing
    };
    let vp = Viewport::fit(&empty_drawing().header.limits, 800, 552);
    let render =
        |loads: &[&str]| flatten_with_libraries(&source(loads), &vp, &libraries).primitives;
    assert_eq!(render(&["S", "T", "S"]), render(&["T", "S"]));
    assert_eq!(render(&["T", "S", "T"]), render(&["S", "T"]));
    assert_ne!(render(&["S", "T", "S"]), render(&["S", "T"]));
}

/// Review pass 2 (h1): incompleteness reasons survive the diagnostic cap.
#[test]
fn incompleteness_reasons_are_kept_outside_the_diagnostic_cap() {
    let missing = |drawing: &mut Drawing| {
        for i in 0..64 {
            drawing
                .items
                .push(Item::Entity(insert(&format!("MISSING{i}"))));
        }
    };
    // 64 distinct missing blocks, then a hidden self-recursive INSERT.
    let mut drawing = empty_drawing();
    drawing.items.push(Item::Block(Block {
        name: "LOOP".into(),
        base: Point { x: 0.0, y: 0.0 },
        entities: vec![insert("LOOP")],
    }));
    missing(&mut drawing);
    drawing.items.push(Item::Entity(Entity::OnLayer {
        layer: 2,
        entity: Box::new(insert("LOOP")),
    }));
    drawing.header.off_layers.insert(2);
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let output = flatten_with_libraries(&drawing, &vp, &Libraries::default());
    assert!(output.incomplete);
    assert!(output
        .diagnostics
        .contains(&"LOAD context: block recursion limit reached".to_string()));

    // 64 distinct missing blocks, then an owner refused by its preflight.
    let mut drawing = empty_drawing();
    missing(&mut drawing);
    drawing.items.push(Item::Repeat(lattice(0, 1)));
    let output = flatten_with_libraries(&drawing, &vp, &Libraries::default());
    assert!(output.incomplete);
    assert!(output
        .diagnostics
        .contains(&"Owner 65 exceeds render work budget".to_string()));
}

/// Hashing a drawing-supplied name is work too: a 64 KB block name costs one
/// unit per 16 bytes at every lookup, and diagnostics quote only a prefix.
#[test]
fn long_names_are_charged_and_shortened_in_diagnostics() {
    let name = "N".repeat(1 << 16);
    let mut drawing = empty_drawing();
    drawing.items = vec![Item::Entity(insert(&name)); 2_000];
    let vp = Viewport::fit(&drawing.header.limits, 800, 552);
    let output = timed("long block names", || {
        flatten_with_libraries(&drawing, &vp, &Libraries::default())
    });
    let stop = output.budget_stop.expect("name hashing is charged");
    // 4,096 units per lookup, at preflight and walk: about 122 owners fit.
    assert_eq!(stop.first_skipped_item, 123, "{stop:?}");
    assert_eq!(
        output.diagnostics[0],
        format!("INSERT: missing block {}...", "N".repeat(40))
    );
}
