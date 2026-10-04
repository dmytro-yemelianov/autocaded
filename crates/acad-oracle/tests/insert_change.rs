//! In-tree original evidence for star INSERT placement and multi-object
//! CHANGE (docs/native-external-insert.md, docs/native-change.md). Prompts
//! are read from the original's final graphics frame with its own display
//! font; drawings come from the original's END. Every original behaviour
//! those documents cite is asserted here, and each `divergence` test pins the
//! measured original result where the native editor deliberately differs.
//! The System image is only read.
#![cfg(unix)]
use acad_model::{Entity, Item, Point};
use acad_oracle::in_tree::{editor_text_rows, generate_dwg_in_tree, observe_in_tree};

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
    eprintln!("skipping in-tree INSERT/CHANGE oracle: extracted System.img absent, NOT validated");
    None
}

fn keys(lines: &[&str], end: bool) -> String {
    let mut keys = String::from("1\rPROBE\r");
    for line in lines {
        keys.push_str(line);
        keys.push('\r');
    }
    if end {
        keys.push_str("END\r");
    }
    keys
}

/// The original's command area (rows 22..=24) after `lines`.
fn screen(disk: &std::path::Path, extra: &[(&str, &[u8])], lines: &[&str]) -> [String; 3] {
    let run = observe_in_tree(
        disk,
        b"",
        extra,
        &[(keys(lines, false).as_bytes(), 400)],
        100_000,
        &[],
    )
    .unwrap();
    assert_eq!(run.stopped, None, "{}", run.console);
    let rows = editor_text_rows(disk, &run.cga).unwrap();
    [rows[22].clone(), rows[23].clone(), rows[24].clone()]
}

/// The prompt the original waits at after `lines`.
fn prompt(disk: &std::path::Path, lines: &[&str]) -> String {
    screen(disk, &[], lines)[2].trim().to_owned()
}

fn original(disk: &std::path::Path, name: &str, lines: &[&str]) -> Vec<Item> {
    let dwg = generate_dwg_in_tree(disk, name, lines)
        .unwrap_or_else(|error| panic!("{name} {lines:?}: {error}"));
    acad_dwg::parse(&dwg).unwrap().items
}

fn rust(lines: &[&str]) -> acad_cmd::Editor {
    let mut editor = acad_cmd::Editor::default();
    for line in lines {
        editor
            .submit(line)
            .unwrap_or_else(|error| panic!("{line}: {error}"));
    }
    editor
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}
fn close_point(a: &Point, b: &Point) -> bool {
    close(a.x, b.x) && close(a.y, b.y)
}
fn close_angle(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(360.0);
    d < 1e-9 || 360.0 - d < 1e-9
}

fn same_entity(a: &Entity, b: &Entity) -> bool {
    match (a, b) {
        (
            Entity::OnLayer { layer, entity },
            Entity::OnLayer {
                layer: other_layer,
                entity: other,
            },
        ) => layer == other_layer && same_entity(entity, other),
        (Entity::Line { start, end }, Entity::Line { start: s, end: e }) => {
            close_point(start, s) && close_point(end, e)
        }
        (
            Entity::Circle { center, radius },
            Entity::Circle {
                center: c,
                radius: r,
            },
        ) => close_point(center, c) && close(*radius, *r),
        (
            Entity::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            },
            Entity::Arc {
                center: c,
                radius: r,
                start_deg: s,
                end_deg: e,
            },
        ) => {
            close_point(center, c)
                && close(*radius, *r)
                && close_angle(*start_deg, *s)
                && close_angle(*end_deg, *e)
        }
        (
            Entity::Trace { p1, p2, p3, p4 },
            Entity::Trace {
                p1: q1,
                p2: q2,
                p3: q3,
                p4: q4,
            },
        ) => {
            close_point(p1, q1) && close_point(p2, q2) && close_point(p3, q3) && close_point(p4, q4)
        }
        (
            Entity::Text {
                origin,
                height,
                rotation_deg,
                value,
            },
            Entity::Text {
                origin: o,
                height: h,
                rotation_deg: r,
                value: v,
            },
        ) => {
            close_point(origin, o)
                && close(*height, *h)
                && close_angle(*rotation_deg, *r)
                && value == v
        }
        (
            Entity::Insert {
                origin,
                x_scale,
                y_scale,
                rotation_deg,
                name,
            },
            Entity::Insert {
                origin: o,
                x_scale: x,
                y_scale: y,
                rotation_deg: r,
                name: n,
            },
        ) => {
            close_point(origin, o)
                && close(*x_scale, *x)
                && close(*y_scale, *y)
                && close_angle(*rotation_deg, *r)
                && name == n
        }
        _ => a == b,
    }
}

fn same_items(a: &[Item], b: &[Item]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|pair| match pair {
            (Item::Entity(a), Item::Entity(b)) | (Item::Erased(a), Item::Erased(b)) => {
                same_entity(a, b)
            }
            (a, b) => a == b,
        })
}

fn assert_same(label: &str, original: &[Item], rust: &[Item]) {
    assert!(
        same_items(original, rust),
        "{label}\noriginal {original:#?}\nrust {rust:#?}"
    );
}

const BLOCK: [&str; 8] = ["LINE", "2,3", "4,5", "", "BLOCK", "B1", "1,2", "LAST"];

fn with(prefix: &[&str], rest: &[&str]) -> Vec<&'static str> {
    prefix
        .iter()
        .chain(rest)
        .map(|line| &*Box::leak(line.to_string().into_boxed_str()))
        .collect()
}

#[test]
fn original_star_insert_asks_no_scale_or_rotation() {
    let Some(disk) = disk() else { return };
    // A plain INSERT asks for the X scale after the point...
    let plain = screen(&disk, &[], &with(&BLOCK, &["INSERT", "B1", "7,8"]));
    assert!(
        plain[2].starts_with(" X scale factor (default=1) or Corner:"),
        "{plain:?}"
    );
    // ...the star form returns to Command: at once.
    let star = screen(&disk, &[], &with(&BLOCK, &["INSERT", "*B1", "7,8"]));
    assert_eq!(star[1].trim(), "Insertion point: 7,8", "{star:?}");
    assert_eq!(star[2], "Command:", "{star:?}");
    // Trailing numbers are commands, not scale answers: the geometry is the
    // pure translation by insertion point minus base, as the native default.
    let lines = with(&BLOCK, &["INSERT", "*B1", "7,8", "2", "3", "30"]);
    let after = screen(&disk, &[], &lines);
    assert!(after[1].contains("Unknown command"), "{after:?}");
    let items = original(&disk, "STARSC", &lines);
    let native = rust(&with(&BLOCK, &["INSERT", "*B1", "7,8"]));
    assert_same("star", &items, &native.drawing().items);
    assert!(native.prompt().starts_with("Command"));
}

#[test]
fn divergence_original_star_insert_aborts_on_s_where_native_offers_placement() {
    let Some(disk) = disk() else { return };
    let star = screen(&disk, &[], &with(&BLOCK, &["INSERT", "*B1", "S"]));
    assert_eq!(star[1], "*Invalid*", "{star:?}");
    assert_eq!(star[2], "Command:", "{star:?}");
    // No original script can rely on `S` at this prompt: the native editor
    // uses it to open the scale/rotation extension.
    let native = rust(&with(&BLOCK, &["INSERT", "*B1", "S"]));
    assert!(native.prompt().contains("X scale"), "{}", native.prompt());
}

#[test]
fn original_star_file_insert_asks_no_scale_and_translates_by_base() {
    let Some(disk) = disk() else { return };
    let part = generate_dwg_in_tree(
        &disk,
        "D2",
        &["LINE", "1,1", "3,2", "", "CIRCLE", "2,2", "0.5"],
    )
    .unwrap();
    let extra: &[(&str, &[u8])] = &[("D2.DWG", &part)];
    let star = screen(&disk, extra, &["INSERT", "*D2", "7,8"]);
    assert_eq!(star[2], "Command:", "{star:?}");
    let run = observe_in_tree(
        &disk,
        b"",
        extra,
        &[(keys(&["INSERT", "*D2", "7,8"], true).as_bytes(), 400)],
        100_000,
        &["PROBE.DWG"],
    )
    .unwrap();
    let saved = &run.created.first().expect("original saved PROBE.DWG").1;
    let items = acad_dwg::parse(saved).unwrap().items;
    let mut editor = acad_cmd::Editor::default();
    editor.submit("INSERT").unwrap();
    editor
        .submit_insert_drawing("*D2", acad_dwg::parse(&part).unwrap())
        .unwrap();
    editor.submit("7,8").unwrap();
    assert!(editor.prompt().starts_with("Command"));
    assert_same("star file", &items, &editor.drawing().items);
}

/// Native prompt in the original's words.
fn native_label(prompt: &str) -> &'static str {
    match prompt {
        p if p.starts_with("CHANGE: new angle") => "New angle:",
        p if p.starts_with("CHANGE TEXT: new height") => "New height:",
        p if p.starts_with("CHANGE TEXT: new angle") => "New angle:",
        p if p.starts_with("CHANGE TEXT: new text") => "New text:",
        p if p.starts_with("Command") => "Command:",
        other => panic!("unexpected native prompt {other}"),
    }
}

/// Compare the prompt after the intersection point and after every
/// answer; the original selects with `window`, the native editor `ALL`
/// (whose window collection continues until Return).
fn prompt_sequence(
    disk: &std::path::Path,
    setup: &[&str],
    window: &[&str],
    answers: &[&str],
    expected: &[&str],
) {
    assert_eq!(answers.len() + 1, expected.len());
    let mut native = rust(&with(setup, &["CHANGE", "ALL"]));
    let mut lines = with(setup, &["CHANGE"]);
    lines.extend(with(window, &[]));
    assert_eq!(prompt(disk, &lines), "Intersection point or (L):");
    for (step, want) in expected.iter().enumerate() {
        let answer = if step == 0 { "3,5" } else { answers[step - 1] };
        lines.push(Box::leak(answer.to_owned().into_boxed_str()));
        native.submit(answer).unwrap();
        assert_eq!(prompt(disk, &lines), *want, "original after {lines:?}");
        assert_eq!(
            native_label(native.prompt()),
            *want,
            "native after {answer}"
        );
    }
}

const MIXED: [&str; 26] = [
    "LINE", "0,0", "1,0", "", "BLOCK", "B1", "0,0", "LAST", "LINE", "1,1", "2,1", "", "CIRCLE",
    "4,1", "0.5", "TEXT", "6,1", "0.5", "0", "AB", "INSERT", "B1", "8,1", "1", "", "",
];
const WINDOW: [&str; 3] = ["W", "0,0", "10,3"];

#[test]
fn original_change_visits_reverse_order_with_one_shared_insert_angle() {
    let Some(disk) = disk() else { return };
    // LINE, CIRCLE, TEXT, INSERT: the INSERT (last drawn) is visited first.
    prompt_sequence(
        &disk,
        &MIXED,
        &WINDOW,
        &["30", "0.25", "60", "XY"],
        &[
            "New angle:",
            "New height:",
            "New angle:",
            "New text:",
            "Command:",
        ],
    );
    // INSERT drawn before TEXT: the TEXT prompts come first.
    prompt_sequence(
        &disk,
        &[
            "LINE", "0,0", "1,0", "", "BLOCK", "B1", "0,0", "LAST", "INSERT", "B1", "4,1", "1", "",
            "", "TEXT", "6,1", "0.5", "0", "T1",
        ],
        &WINDOW,
        &["", "", "", ""],
        &[
            "New height:",
            "New angle:",
            "New text:",
            "New angle:",
            "Command:",
        ],
    );
    // Two INSERTs of different blocks share one angle prompt.
    prompt_sequence(
        &disk,
        &TWO_BLOCKS,
        &WINDOW,
        &["11"],
        &["New angle:", "Command:"],
    );
}

const TWO_BLOCKS: [&str; 28] = [
    "LINE", "0,0", "1,0", "", "BLOCK", "B1", "0,0", "LAST", "LINE", "0,0", "0,1", "", "BLOCK",
    "B2", "0,0", "LAST", "INSERT", "B1", "4,1", "1", "", "10", "INSERT", "B2", "6,1", "1", "",
    "20",
];

#[test]
fn original_shared_insert_angle_applies_to_all_and_blank_keeps_each() {
    let Some(disk) = disk() else { return };
    for (name, angle, expected) in [
        ("CHGANG1", "11", [11.0, 11.0]),
        ("CHGANG2", "", [10.0, 20.0]),
    ] {
        let mut lines = with(&TWO_BLOCKS, &["CHANGE"]);
        lines.extend(with(&WINDOW, &["3,5", angle]));
        let items = original(&disk, name, &lines);
        let native = rust(&with(&TWO_BLOCKS, &["CHANGE", "ALL", "3,5", angle]));
        assert_same(name, &items, &native.drawing().items);
        let angles: Vec<f64> = items[4..]
            .iter()
            .map(|item| match item {
                Item::Entity(Entity::OnLayer { entity, .. }) => match entity.as_ref() {
                    Entity::Insert {
                        rotation_deg,
                        origin,
                        ..
                    } => {
                        assert!(close_point(origin, &Point { x: 3.0, y: 5.0 }));
                        *rotation_deg
                    }
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            })
            .collect();
        assert!(
            close(angles[0], expected[0]) && close(angles[1], expected[1]),
            "{angles:?}"
        );
    }
}

const INTERLEAVED: [&str; 34] = [
    "LINE", "0,0", "1,0", "", "BLOCK", "B1", "0,0", "LAST", "TEXT", "2,1", "0.5", "0", "T1",
    "INSERT", "B1", "4,1", "1", "", "", "TEXT", "6,1", "0.5", "0", "T2", "INSERT", "B1", "8,1",
    "1", "", "", "CHANGE", "W", "0,0", "10,3",
];

#[test]
fn original_change_interleaved_texts_and_inserts_match_native() {
    let Some(disk) = disk() else { return };
    let answers = ["3,5", "11", "12", "13", "14", "15", "16", "17"];
    prompt_sequence(
        &disk,
        &INTERLEAVED[..30],
        &WINDOW,
        &answers[1..],
        &[
            "New angle:",
            "New height:",
            "New angle:",
            "New text:",
            "New height:",
            "New angle:",
            "New text:",
            "Command:",
        ],
    );
    let items = original(&disk, "CHGMIX", &with(&INTERLEAVED, &answers));
    let mut native = rust(&with(&INTERLEAVED[..30], &["CHANGE", "ALL"]));
    for answer in answers {
        native.submit(answer).unwrap();
    }
    // T2 (visited first) took 12/13/"14", T1 15/16/"17"; both INSERTs 11.
    // Changed TEXT records stay erased; new values append in visiting order.
    assert_same("interleaved", &items, &native.drawing().items);
    assert!(matches!(items[2], Item::Erased(_)) && matches!(items[4], Item::Erased(_)));
}

#[test]
fn original_change_mixed_selection_geometry_matches_native() {
    let Some(disk) = disk() else { return };
    for (name, answers) in [
        ("CHGMIXA", &["3,5", "30", "0.25", "60", "XY"][..]),
        // Blank intersection point keeps every location; properties still apply.
        ("CHGMIXB", &["", "30", "", "", ""][..]),
        // Any non-blank text, even the old value, erases and appends.
        ("CHGMIXC", &["", "", "", "", "AB"][..]),
    ] {
        let mut lines = with(&MIXED, &["CHANGE"]);
        lines.extend(with(&WINDOW, answers));
        let items = original(&disk, name, &lines);
        let native = rust(&with(&with(&MIXED, &["CHANGE", "ALL"]), answers));
        assert_same(name, &items, &native.drawing().items);
    }
}

#[test]
fn original_change_leaves_other_kinds_unchanged() {
    let Some(disk) = disk() else { return };
    let setup = [
        "ARC", "4,3", "3,4", "2,3", "POINT", "5,1", "TRACE", "0.1", "1,4", "2,4", "", "SOLID",
        "5,4", "6,4", "5,4.5", "6,4.5", "", "LINE", "7,1", "8,1", "",
    ];
    let mut lines = with(&setup, &["CHANGE", "W", "0,0", "10,5", "3,5"]);
    let items = original(&disk, "CHGKIND", &lines);
    let native = rust(&with(&setup, &["CHANGE", "ALL", "3,5"]));
    assert_same("kinds", &items, &native.drawing().items);
    let Item::Entity(Entity::OnLayer { entity, .. }) = items.last().unwrap() else {
        panic!()
    };
    assert!(matches!(
        entity.as_ref(),
        Entity::Line {
            start: Point { x: 3.0, y: 5.0 },
            ..
        }
    ));
    lines.truncate(setup.len());
    let unchanged = original(&disk, "CHGNONE", &lines);
    assert!(same_items(
        &items[..items.len() - 1],
        &unchanged[..unchanged.len() - 1]
    ));
}

#[test]
fn divergence_original_change_edits_repeat_members_where_native_refuses_groups() {
    let Some(disk) = disk() else { return };
    let setup = [
        "REPEAT", "LINE", "1,1", "2,1", "", "ENDREP", "2", "2", "0.5", "0.5", "LINE", "7,1", "8,1",
        "",
    ];
    let items = original(
        &disk,
        "CHGREP",
        &with(&setup, &["CHANGE", "W", "0,0", "10,5", "3,5"]),
    );
    let Item::Repeat(repeat) = &items[0] else {
        panic!("{items:?}")
    };
    assert!(matches!(&repeat.entities[0], Entity::OnLayer { entity, .. }
        if matches!(entity.as_ref(), Entity::Line { end: Point { x: 3.0, y: 5.0 }, .. })));
    // Native group rule (docs/native-selection-contract.md): refuse, unchanged.
    let mut native = rust(&with(&setup, &["CHANGE", "ALL"]));
    let before = native.drawing().clone();
    assert!(native.submit("3,5").is_err());
    assert_eq!(native.drawing(), &before);
}

#[test]
fn original_change_without_a_new_value_rewrites_the_text_in_place() {
    let Some(disk) = disk() else { return };
    let setup = ["TEXT", "6,1", "0.5", "0", "AB"];
    let answers = ["3,5", "0.25", "60", ""];
    let mut lines = with(&setup, &["CHANGE"]);
    lines.extend(with(&WINDOW, &answers));
    let items = original(&disk, "CHGH", &lines);
    assert_eq!(items.len(), 1, "{items:?}");
    let Item::Entity(Entity::OnLayer { entity, .. }) = &items[0] else {
        panic!("TEXT stays live at its index: {items:?}")
    };
    assert!(same_entity(
        entity,
        &Entity::Text {
            origin: Point { x: 3.0, y: 5.0 },
            height: 0.25,
            rotation_deg: 60.0,
            value: "AB".into(),
        }
    ));
    let native = rust(&with(&with(&setup, &["CHANGE", "ALL"]), &answers));
    assert_same("CHGH", &items, &native.drawing().items);
}

/// Replay natively; OOPS without an erase set may be refused natively.
fn rust_lenient(lines: &[&str]) -> acad_cmd::Editor {
    let mut editor = acad_cmd::Editor::default();
    for line in lines {
        if editor.submit(line).is_err() {
            assert_eq!(*line, "OOPS", "only OOPS may be refused");
            editor.cancel_command().unwrap();
        }
    }
    editor
}

#[test]
fn original_oops_after_change_restores_the_last_erase_and_never_the_changed_text() {
    let Some(disk) = disk() else { return };
    // ERASE L, CHANGE with a new value, OOPS: the erased LINE returns, the
    // TEXT erased by CHANGE stays erased beside its appended replacement.
    let setup = [
        "TEXT", "6,1", "0.5", "0", "AB", "LINE", "1,1", "2,1", "", "ERASE", "L",
    ];
    let answers = ["", "", "", "CD", "OOPS"];
    let mut lines = with(&setup, &["CHANGE"]);
    lines.extend(with(&WINDOW, &answers));
    let items = original(&disk, "CHGO", &lines);
    assert!(
        matches!(items[0], Item::Erased(_)) && matches!(items[1], Item::Entity(_)),
        "{items:?}"
    );
    assert_eq!(items.len(), 3, "{items:?}");
    let native = rust_lenient(&with(&with(&setup, &["CHANGE", "ALL"]), &answers));
    assert_same("CHGO", &items, &native.drawing().items);
    // OOPS straight after CHANGE does not revive the old TEXT either.
    let lines = [
        "TEXT", "6,1", "0.5", "0", "AB", "CHANGE", "L", "", "", "", "CD", "OOPS",
    ];
    let items = original(&disk, "CHGO2", &lines);
    assert_eq!(items.len(), 2, "{items:?}");
    assert!(matches!(items[0], Item::Erased(_)), "{items:?}");
    assert_same("CHGO2", &items, &rust_lenient(&lines).drawing().items);
}

#[test]
fn original_blank_point_measures_an_insert_angle_point_from_the_insert() {
    let Some(disk) = disk() else { return };
    let lines = [
        "LINE", "0,0", "1,0", "", "BLOCK", "B1", "0,0", "LAST", "INSERT", "B1", "4,1", "1", "", "",
        "CHANGE", "L", "", "5,2",
    ];
    let items = original(&disk, "CHGIA", &lines);
    let Item::Entity(Entity::OnLayer { entity, .. }) = items.last().unwrap() else {
        panic!()
    };
    assert!(
        matches!(entity.as_ref(), Entity::Insert { origin: Point { x: 4.0, y: 1.0 }, rotation_deg, .. }
        if close_angle(*rotation_deg, 45.0)),
        "{entity:?}"
    );
    assert_same("CHGIA", &items, &rust(&lines).drawing().items);
}
