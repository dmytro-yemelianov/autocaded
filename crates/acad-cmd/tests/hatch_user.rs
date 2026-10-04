//! HATCH `U` (user-defined) patterns and external pattern files
//! (docs/native-hatch-user.md).
use acad_cmd::{Editor, Effect};
use acad_model::{Entity, Item, Point};

const PATTERNS: [&str; 23] = [
    "EARTH", "ESCHER", "FLEX", "GRASS", "GRATE", "HEX", "HONEY", "HOUND", "INSUL", "LINE", "MUDST",
    "NET", "NET3", "PLAST", "PLASTI", "SACNCR", "SQUARE", "STARS", "STEEL", "SWAMP", "TRANS",
    "TRIANG", "ZIGZAG",
];
const ANGLE: &str = "HATCH: angle for crosshatch lines";
const SPACING: &str = "HATCH: spacing between lines";
const DOUBLE: &str = "HATCH: double hatch area (Y/N) <N>";
const SELECT: &str = "HATCH: select objects on Window or Last";

fn inputs(editor: &mut Editor, values: &[&str]) {
    for value in values {
        editor
            .submit(value)
            .unwrap_or_else(|e| panic!("{value}: {e}"));
    }
}
fn square(editor: &mut Editor, lo: f64, hi: f64) {
    inputs(
        editor,
        &[
            "LINE",
            &format!("{lo},{lo}"),
            &format!("{hi},{lo}"),
            &format!("{hi},{hi}"),
            &format!("{lo},{hi}"),
            "C",
        ],
    );
}
fn nested() -> Editor {
    let mut editor = Editor::default();
    for (lo, hi) in [(0.0, 12.0), (2.0, 10.0), (4.0, 8.0)] {
        square(&mut editor, lo, hi);
    }
    editor
}
fn hatched(mut editor: Editor, answers: &[&str]) -> Editor {
    editor.submit("HATCH").unwrap();
    inputs(&mut editor, answers);
    editor.submit("ALL").unwrap();
    assert_eq!(editor.prompt(), "Command");
    editor
}

#[test]
fn user_pattern_prompts_follow_the_help_file_without_a_scale_prompt() {
    let mut editor = nested();
    editor.submit("HATCH").unwrap();
    editor.submit("u").unwrap();
    assert_eq!(editor.prompt(), ANGLE);
    assert!(editor.accepts_mouse_point());
    editor.submit("45").unwrap();
    assert_eq!(editor.prompt(), SPACING);
    assert!(editor.accepts_mouse_point());
    editor.submit("1").unwrap();
    assert_eq!(editor.prompt(), DOUBLE);
    assert!(!editor.accepts_mouse_point());
    editor.submit("").unwrap();
    assert_eq!(editor.prompt(), SELECT);
    editor.submit("ALL").unwrap();
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing().blocks().count(), 1);
}

#[test]
fn user_pattern_is_scaled_line_and_double_is_scaled_net() {
    for (angle, spacing) in [("0", 1.0), ("45", 0.5), ("-30", 0.75), ("400", 2.0)] {
        let scale = (spacing / 0.125).to_string();
        for (double, named) in [("N", "LINE"), ("", "LINE"), ("NO", "LINE"), ("Y", "NET")] {
            let user = hatched(nested(), &["U", angle, &spacing.to_string(), double]);
            let reference = hatched(nested(), &[named, &scale, angle]);
            assert_eq!(user.drawing(), reference.drawing(), "{angle} {double}");
        }
        let yes = hatched(nested(), &["U", angle, &spacing.to_string(), "yes"]);
        let net = hatched(nested(), &["NET", &scale, angle]);
        assert_eq!(yes.drawing(), net.drawing());
    }
}

#[test]
fn user_pattern_composes_with_island_styles() {
    for style in ["N", "O", "I"] {
        let user = hatched(nested(), &[&format!("U,{style}"), "30", "0.5", "Y"]);
        let reference = hatched(nested(), &[&format!("NET,{style}"), "4", "30"]);
        assert_eq!(user.drawing(), reference.drawing(), "{style}");
    }
    let outer = hatched(nested(), &["U , o", "0", "1", "N"]);
    let ignore = hatched(nested(), &["U,I", "0", "1", "N"]);
    assert_ne!(outer.drawing(), ignore.drawing());
    let mut editor = nested();
    let before = editor.drawing().clone();
    for input in ["U,X", "U,", "U,OI"] {
        editor.submit("HATCH").unwrap();
        assert!(editor.submit(input).is_err(), "{input}");
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn angle_and_spacing_accept_two_points_and_mouse_picks() {
    let typed = hatched(nested(), &["U", "45", "1", "N"]);
    let points = hatched(nested(), &["U", "1,1", "2,2", "2,3", "2,4", "N"]);
    assert_eq!(points.drawing(), typed.drawing());
    let relative = hatched(nested(), &["U", "1,1", "@1,1", "2,3", "@0,-1", "N"]);
    assert_eq!(relative.drawing(), typed.drawing());

    let mut editor = nested();
    inputs(&mut editor, &["HATCH", "U", "1,1"]);
    assert_eq!(editor.prompt(), "HATCH: angle second point");
    assert!(editor.accepts_mouse_point());
    editor.submit_mouse_point(Point { x: 2.0, y: 2.0 }).unwrap();
    editor.submit_mouse_point(Point { x: 2.0, y: 3.0 }).unwrap();
    assert_eq!(editor.prompt(), "HATCH: spacing second point");
    editor.submit_mouse_point(Point { x: 2.0, y: 4.0 }).unwrap();
    inputs(&mut editor, &["N", "ALL"]);
    assert_eq!(editor.drawing(), typed.drawing());
}

#[test]
fn invalid_answers_are_retried_at_the_same_prompt() {
    let mut editor = nested();
    let before = editor.drawing().clone();
    inputs(&mut editor, &["HATCH", "U"]);
    for bad in ["", "abc", "inf", "NaN", "1e999"] {
        assert!(editor.submit(bad).is_err(), "angle {bad:?}");
        assert_eq!(editor.prompt(), ANGLE);
    }
    editor.submit("1,1").unwrap();
    for bad in ["1,1", "abc", "inf,0"] {
        assert!(editor.submit(bad).is_err(), "angle point {bad:?}");
        assert_eq!(editor.prompt(), "HATCH: angle second point");
    }
    editor.submit("2,1").unwrap();
    assert_eq!(editor.prompt(), SPACING);
    for bad in ["", "0", "-0", "-1", "abc", "inf", "1e999"] {
        assert!(editor.submit(bad).is_err(), "spacing {bad:?}");
        assert_eq!(editor.prompt(), SPACING);
    }
    editor.submit("3,3").unwrap();
    assert_eq!(editor.prompt(), "HATCH: spacing second point");
    assert!(editor.submit("3,3").is_err());
    assert_eq!(editor.prompt(), SPACING);
    editor.submit("0.5").unwrap();
    for bad in ["X", "1", "YN", "maybe"] {
        assert!(editor.submit(bad).is_err(), "double {bad:?}");
        assert_eq!(editor.prompt(), DOUBLE);
    }
    assert_eq!(editor.drawing(), &before);
    inputs(&mut editor, &["y", "ALL"]);
    assert_eq!(
        editor.drawing(),
        hatched(nested(), &["U", "0", "0.5", "Y"]).drawing()
    );
}

#[test]
fn cancel_at_any_user_prompt_leaves_no_block_or_undo_step() {
    let original = nested();
    let mut previous = original.clone();
    previous.submit("UNDO").unwrap();
    let steps: [&[&str]; 7] = [
        &["U"],
        &["U", "1,1"],
        &["U", "0"],
        &["U", "0", "1,1"],
        &["U", "0", "1"],
        &["U", "0", "1", "Y"],
        &["U", "0", "1", "Y", "W", "-1,-1"],
    ];
    for answers in steps {
        let mut editor = original.clone();
        editor.submit("HATCH").unwrap();
        inputs(&mut editor, answers);
        editor.cancel_command().unwrap();
        assert_eq!(editor.prompt(), "Command", "{answers:?}");
        assert_eq!(editor.drawing(), original.drawing(), "{answers:?}");
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), previous.drawing(), "{answers:?}");
    }
}

#[test]
fn user_hatch_is_one_undo_step_and_survives_dwg() {
    let original = nested();
    let editor = hatched(original.clone(), &["U,O", "15", "0.25", "Y"]);
    assert_eq!(editor.drawing().blocks().count(), 1);
    let dwg = acad_dwg::parse(&acad_dwg::write(editor.drawing()).unwrap()).unwrap();
    assert_eq!(dwg.items, editor.drawing().items);
    let mut editor = editor;
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), original.drawing());
}

#[test]
fn user_budget_exhaustion_is_atomic_with_retry_and_cancel() {
    let mut original = Editor::default();
    square(&mut original, 0.0, 12.0);
    square(&mut original, 20.0, 21.0);
    let before = original.drawing().clone();
    let mut previous = original.clone();
    previous.submit("UNDO").unwrap();

    let mut editor = original.clone();
    inputs(&mut editor, &["HATCH", "U,I", "0", "0.0001", "Y"]);
    let error = editor.submit("ALL").unwrap_err();
    assert!(
        error.contains("limit") || error.contains("sweep"),
        "{error}"
    );
    assert_eq!(editor.drawing(), &before);
    assert_eq!(editor.prompt(), SELECT);
    // 1 / 0.0001 = 10,000 rows per family on the small square fits.
    inputs(&mut editor, &["W", "19,19", "22,22"]);
    assert_eq!(editor.prompt(), "Command");
    assert_eq!(editor.drawing().blocks().count(), 1);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), &before);

    let mut editor = original.clone();
    inputs(&mut editor, &["HATCH", "U", "0", "0.0001", "Y"]);
    assert!(editor.submit("ALL").is_err());
    editor.cancel_command().unwrap();
    assert_eq!(editor.drawing(), &before);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), previous.drawing());
}

// External pattern files.

const FILE: &str = "\
;; a comment line
*other,Not this one
0, 0,0, 0,1

*Rails, two rails and dots
0, 0,0, 0,.5
  90 , .25 , 0 , .5 , 1 , 0 , -.5
*rails,a later duplicate is ignored
45, 0,0, 0,1
\x1a*ignored,after DOS end of file
not, a, row
";

fn at_pattern_prompt() -> Editor {
    let mut editor = nested();
    editor.submit("HATCH").unwrap();
    editor
}

fn load(editor: &mut Editor, input: &str, contents: &[u8]) -> Result<Effect, String> {
    editor.submit_hatch_pattern_file(input, "TEST.PAT", contents)
}

#[test]
fn only_unknown_names_at_the_pattern_prompt_request_a_file() {
    let mut editor = nested();
    assert_eq!(editor.hatch_pattern_file_request("RAILS"), None);
    editor.submit("HATCH").unwrap();
    for builtin in ["LINE", "net,o", " Zigzag ", "U", "u,I", "?", ""] {
        assert_eq!(
            editor.hatch_pattern_file_request(builtin),
            None,
            "{builtin}"
        );
    }
    assert_eq!(
        editor.hatch_pattern_file_request(" Rails ,O").as_deref(),
        Some("Rails")
    );
    assert_eq!(
        editor
            .hatch_pattern_file_request("D:pats/Rails.pat")
            .as_deref(),
        Some("D:pats/Rails.pat")
    );
    // An invalid style is still rejected by the ordinary prompt.
    assert_eq!(editor.hatch_pattern_file_request("RAILS,X"), None);
    // Without a Session the name stays unknown.
    assert!(editor
        .submit("RAILS")
        .unwrap_err()
        .contains("unknown HATCH pattern"));
}

#[test]
fn file_patterns_parse_the_acad_pat_syntax_and_use_the_named_definition() {
    let mut editor = at_pattern_prompt();
    load(&mut editor, "rails", FILE.as_bytes()).unwrap();
    assert_eq!(editor.prompt(), "HATCH: scale for pattern {1}");
    inputs(&mut editor, &["2", "0", "ALL"]);
    let block = editor.drawing().blocks().next().unwrap();
    let mut lines = 0;
    let mut points = 0;
    for entity in &block.entities {
        let Entity::OnLayer { layer: 127, entity } = entity else {
            panic!("layer 127")
        };
        match entity.as_ref() {
            Entity::Line { start, end } if start.y == end.y => {
                assert_eq!(start.y.rem_euclid(1.0), 0.0);
                lines += 1;
            }
            Entity::Line { .. } => lines += 1,
            Entity::Point { .. } => points += 1,
            other => panic!("{other:?}"),
        }
    }
    assert!(lines > 0 && points > 0, "{lines} {points}");
    // A path spec looks up the file's base name, with the style suffix.
    let mut styled = at_pattern_prompt();
    load(&mut styled, "D:pats/Rails.pat,O", FILE.as_bytes()).unwrap();
    inputs(&mut styled, &["2", "0", "ALL"]);
    assert_eq!(styled.drawing().blocks().count(), 1);
    assert_ne!(styled.drawing(), editor.drawing());
}

#[test]
fn file_pattern_errors_name_the_line_and_change_nothing() {
    let cases: [(&str, &str); 12] = [
        (
            "*P\n0, 0,0, 0\n",
            "line 2: expected angle, x-origin, y-origin, delta-x, delta-y",
        ),
        ("*P\n0, 0,0, 0,x\n", "line 2: invalid number: x"),
        ("*P\n0, 0,0, 0,inf\n", "line 2: values must be finite"),
        ("*P\n0, 0,0, 0,0\n", "line 2: delta-y must be positive"),
        ("*P\n0, 0,0, 0,-1\n", "line 2: delta-y must be positive"),
        (
            "*P\r\n\r\n0, 0,0, 0,1, 0,0\r\n",
            "line 3: dash entries must not all be zero",
        ),
        ("*P\n0, 0,0, 0,1,\n", "line 2: invalid number: "),
        ("*P\n", "*P has no rows"),
        ("*Q\n0, 0,0, 0,1\n", "TEST.PAT has no *P definition"),
        (
            "*P\n0, 0,0, 0,1\n0, 0,0, 0,1, \u{e9}\n",
            "line 3: not ASCII text",
        ),
        (
            "*P\n0, 0,0, 0,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1\n",
            "line 2: more than 16 dash entries",
        ),
        ("*P\n0, 0,0, 0,1, 0.5,-0.5\n*P2\n0, 0, 0\n", ""),
    ];
    for (contents, expected) in cases {
        let mut editor = at_pattern_prompt();
        let before = editor.drawing().clone();
        let result = load(&mut editor, "P", contents.as_bytes());
        if expected.is_empty() {
            result.unwrap();
            continue;
        }
        let error = result.unwrap_err();
        assert!(error.contains(expected), "{contents:?}: {error}");
        assert!(error.contains("TEST.PAT"), "{error}");
        assert_eq!(editor.prompt(), "Command", "{contents:?}");
        assert_eq!(editor.drawing(), &before);
    }
}

#[test]
fn file_pattern_limits_bound_rows_lines_and_bytes() {
    let rows = |count: usize| {
        let mut text = String::from("*P\n");
        for _ in 0..count {
            text.push_str("0, 0,0, 0,1\n");
        }
        text
    };
    let mut editor = at_pattern_prompt();
    load(&mut editor, "P", rows(64).as_bytes()).unwrap();
    let mut editor = at_pattern_prompt();
    let error = load(&mut editor, "P", rows(65).as_bytes()).unwrap_err();
    assert!(error.contains("line 66: more than 64 rows"), "{error}");

    let mut text = "\n".repeat(8_191);
    text.push_str("*P\n0, 0,0, 0,1\n");
    let mut editor = at_pattern_prompt();
    let error = load(&mut editor, "P", text.as_bytes()).unwrap_err();
    assert!(error.contains("more than 8192 lines"), "{error}");

    let mut big = rows(1).into_bytes();
    big.resize(acad_cmd::MAX_HATCH_PATTERN_FILE_BYTES + 1, b'\n');
    let mut editor = at_pattern_prompt();
    let error = load(&mut editor, "P", &big).unwrap_err();
    assert!(error.contains("262144-byte"), "{error}");

    // Not at the pattern prompt.
    let mut editor = nested();
    assert!(load(&mut editor, "P", rows(1).as_bytes()).is_err());
    assert_eq!(editor.prompt(), "Command");
}

#[test]
fn file_pattern_budget_failure_and_cancel_are_atomic_and_one_undo() {
    let original = nested();
    let mut previous = original.clone();
    previous.submit("UNDO").unwrap();
    let mut editor = original.clone();
    editor.submit("HATCH").unwrap();
    load(&mut editor, "rails,I", FILE.as_bytes()).unwrap();
    inputs(&mut editor, &["0.00001", "0"]);
    assert!(editor.submit("ALL").is_err());
    assert_eq!(editor.drawing(), original.drawing());
    editor.cancel_command().unwrap();
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), previous.drawing());

    let mut editor = original.clone();
    editor.submit("HATCH").unwrap();
    load(&mut editor, "rails", FILE.as_bytes()).unwrap();
    inputs(&mut editor, &["1", "0", "ALL"]);
    assert_eq!(editor.drawing().blocks().count(), 1);
    let dwg = acad_dwg::parse(&acad_dwg::write(editor.drawing()).unwrap()).unwrap();
    assert_eq!(dwg.items, editor.drawing().items);
    editor.submit("UNDO").unwrap();
    assert_eq!(editor.drawing(), original.drawing());
}

/// The retained ACAD.PAT reproduces every built-in definition exactly.
#[test]
fn retained_acad_pat_matches_all_built_in_patterns() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/System/ACAD.PAT");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipping: retained corpus/System/ACAD.PAT absent");
        return;
    };
    for pattern in PATTERNS {
        for (scale, angle) in [("1", "0"), ("2", "30")] {
            let built_in = hatched(nested(), &[pattern, scale, angle]);
            let mut editor = at_pattern_prompt();
            editor
                .submit_hatch_pattern_file(pattern, "ACAD.PAT", &bytes)
                .unwrap();
            inputs(&mut editor, &[scale, angle, "ALL"]);
            assert_eq!(editor.drawing(), built_in.drawing(), "{pattern} {scale}");
        }
    }
    // ACAD.PAT-only names are reachable through the file route.
    let mut editor = at_pattern_prompt();
    editor
        .submit_hatch_pattern_file("ansi31,o", "ACAD.PAT", &bytes)
        .unwrap();
    inputs(&mut editor, &["", "", "ALL"]);
    let Item::Block(block) = &editor.drawing().items[editor.drawing().items.len() - 2] else {
        panic!("hatch block")
    };
    assert!(!block.entities.is_empty());
}

fn many_rows(row: &str, count: usize) -> String {
    let mut text = String::from("*P\n");
    for _ in 0..count {
        text.push_str(row);
        text.push('\n');
    }
    text
}

#[test]
fn gaps_only_rows_are_rejected_but_dots_and_continuous_rows_parse() {
    let mut editor = at_pattern_prompt();
    let error = load(
        &mut editor,
        "P",
        many_rows("0, 0,0, 0,.00004, -.00004", 64).as_bytes(),
    )
    .unwrap_err();
    assert!(
        error.contains("line 2: dash entries need a stroke"),
        "{error}"
    );
    assert_eq!(editor.prompt(), "Command");
    for row in ["0, 0,0, 0,1", "0, 0,0, 0,1, 0,-.5", "0, 0,0, 0,1, -.5,.25"] {
        let mut editor = at_pattern_prompt();
        load(&mut editor, "P", many_rows(row, 1).as_bytes()).unwrap();
    }
}

/// Rows whose strokes vanish below float precision emit nothing, so only the
/// total work bound (rows plus dash cycles visited) stops them, promptly and
/// atomically.
#[test]
fn invisible_stroke_files_exhaust_the_work_budget_promptly_and_atomically() {
    let mut original = Editor::default();
    square(&mut original, 100.0, 104.0);
    let mut previous = original.clone();
    previous.submit("UNDO").unwrap();
    for row in [
        "0, 0,0, 0,.00004, -.00002,1e-18,-.00002",
        "0, 0,0, 0,.00004, -.00004,1e-17",
    ] {
        let mut editor = original.clone();
        editor.submit("HATCH").unwrap();
        load(&mut editor, "P,I", many_rows(row, 64).as_bytes()).unwrap();
        inputs(&mut editor, &["1", "0"]);
        let started = std::time::Instant::now();
        let error = editor.submit("ALL").unwrap_err();
        assert!(
            started.elapsed().as_secs_f64() < 2.0,
            "{:?}",
            started.elapsed()
        );
        assert!(
            error.contains("HATCH work exceeds the limit of 10000000"),
            "{error}"
        );
        assert_eq!(editor.drawing(), original.drawing());
        assert_eq!(editor.prompt(), SELECT);
        editor.cancel_command().unwrap();
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), previous.drawing());
    }
}

/// 2,000 tiny squares (8,000 edges) near y = 0 plus one square near y = 1000,
/// hatched with 21 continuous families of spacing 0.0100001 (the ESCHER
/// family count): ~100,000 rows per family. Each row must visit only the
/// edges whose extent it can cross, not all 8,000 (review h2 pass 2 P3:
/// 58 s in release before the edge index).
fn dense_boundary() -> Editor {
    let mut editor = Editor::default();
    let mut square = |x: f64, y: f64, size: f64| {
        let corners = [
            Point { x, y },
            Point { x: x + size, y },
            Point {
                x: x + size,
                y: y + size,
            },
            Point { x, y: y + size },
        ];
        for index in 0..4 {
            editor.drawing_mut().items.push(Item::Entity(Entity::Line {
                start: corners[index],
                end: corners[(index + 1) % 4],
            }));
        }
    };
    for index in 0..2000 {
        square(index as f64 * 0.01, 0.0, 0.004);
    }
    square(0.0, 999.0, 0.5);
    editor
}

#[test]
fn dense_boundaries_hatch_responsively_with_many_families() {
    for style in ["", ",O", ",I"] {
        let mut editor = dense_boundary();
        editor.submit("HATCH").unwrap();
        load(
            &mut editor,
            &format!("P{style}"),
            many_rows("0, 0,0, 0,.0100001", 21).as_bytes(),
        )
        .unwrap();
        inputs(&mut editor, &["1", "0"]);
        let started = std::time::Instant::now();
        editor.submit("ALL").unwrap();
        let elapsed = started.elapsed();
        eprintln!("dense boundary{style}: {elapsed:?}");
        assert_eq!(editor.prompt(), "Command");
        let block = editor.drawing().blocks().next().unwrap();
        // Row 0 crosses every tiny square once; the top square adds its rows.
        assert_eq!(block.entities.len(), 21 * (2000 + 50), "{style}");
        assert!(elapsed.as_secs_f64() < 10.0, "{style}: {elapsed:?}");
    }
}
