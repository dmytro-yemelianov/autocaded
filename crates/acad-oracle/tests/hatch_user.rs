//! In-tree original anchors for HATCH `U` and the run-time `ACAD.PAT`
//! catalogue (docs/native-hatch-user.md). Each script runs the original
//! ACAD.EXE with the in-tree 8086/DOS runner. `compare` cases must agree
//! with the Rust editor in record kinds, layers and geometry to 1e-6 in
//! record order; the divergence tests pin the measured original result where
//! the native editor deliberately differs.
#![cfg(unix)]
use acad_model::{Entity, Item, Point};
use std::path::{Path, PathBuf};

fn disk() -> Option<PathBuf> {
    let disk = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() {
        Some(disk)
    } else {
        eprintln!("skipping in-tree oracle: extracted System.img absent");
        None
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}
fn close_point(a: &Point, b: &Point) -> bool {
    close(a.x, b.x) && close(a.y, b.y)
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
        (Entity::Point { origin }, Entity::Point { origin: o }) => close_point(origin, o),
        _ => a == b,
    }
}

fn same_item(a: &Item, b: &Item) -> bool {
    match (a, b) {
        (Item::Entity(a), Item::Entity(b)) => same_entity(a, b),
        (Item::Block(a), Item::Block(b)) => {
            a.name == b.name
                && close_point(&a.base, &b.base)
                && a.entities.len() == b.entities.len()
                && a.entities
                    .iter()
                    .zip(&b.entities)
                    .all(|(a, b)| same_entity(a, b))
        }
        _ => a == b,
    }
}

fn assert_same(name: &str, original: &[Item], rust: &[Item]) {
    assert!(
        original.len() == rust.len() && original.iter().zip(rust).all(|(a, b)| same_item(a, b)),
        "{name}\noriginal {original:#?}\nrust {rust:#?}"
    );
}

fn original_on(disk: &Path, name: &str, inputs: &[&str]) -> Vec<Item> {
    let dwg = acad_oracle::generate_dwg_in_tree(disk, name, inputs)
        .unwrap_or_else(|error| panic!("{name} {inputs:?}: {error}"));
    acad_dwg::parse(&dwg).unwrap().items
}

fn original(name: &str, inputs: &[&str]) -> Option<Vec<Item>> {
    Some(original_on(&disk()?, name, inputs))
}

fn rust(name: &str, inputs: &[&str]) -> acad_cmd::Editor {
    let mut editor = acad_cmd::Editor::default();
    for input in inputs {
        editor
            .submit(input)
            .unwrap_or_else(|error| panic!("{name} {input}: {error}"));
    }
    editor
}

fn hatch_blocks(items: &[Item]) -> usize {
    items
        .iter()
        .filter(|item| matches!(item, Item::Block(_)))
        .count()
}

const SQUARE: [&str; 7] = ["LINE", "1,1", "5,1", "5,5", "1,5", "1,1", ""];
const WINDOW: [&str; 3] = ["W", "0,0", "6,6"];

fn square_hatch<'a>(answers: &[&'a str]) -> Vec<&'a str> {
    let mut inputs = SQUARE.to_vec();
    inputs.push("HATCH");
    inputs.extend_from_slice(answers);
    inputs.extend_from_slice(&WINDOW);
    inputs
}

#[test]
fn original_user_patterns_match_native_geometry() {
    let cases: [(&str, Vec<&str>); 9] = [
        ("UANGLE", square_hatch(&["U", "45", "1", "N"])),
        ("UDOUBLE", square_hatch(&["U", "0", "1", "Y"])),
        ("UROT", square_hatch(&["U", "30", "1.5", "YES"])),
        ("ULOWER", square_hatch(&["u", "45", "1", ""])),
        ("UNO", square_hatch(&["U", "0", "1", "NO"])),
        ("UY", square_hatch(&["U", "60", "0.5", "y"])),
        ("USTYLE", square_hatch(&["U,O", "0", "1", "N"])),
        (
            "UPOINTS",
            square_hatch(&["U", "1,1", "2,2", "2,3", "2,4", "N"]),
        ),
        (
            "UCIRCLE",
            vec![
                "CIRCLE", "3,3", "2", "HATCH", "U", "-30", "0.75", "Y", "W", "0,0", "6,6",
            ],
        ),
    ];
    for (name, inputs) in cases {
        let Some(original) = original(name, &inputs) else {
            return;
        };
        assert_eq!(hatch_blocks(&original), 1, "{name}");
        assert_same(name, &original, &rust(name, &inputs).drawing().items);
    }
}

#[test]
fn original_double_hatch_emits_the_perpendicular_family_second() {
    let inputs = square_hatch(&["U", "0", "1", "Y"]);
    let Some(original) = original("UORDER", &inputs) else {
        return;
    };
    let Item::Block(block) = &original[4] else {
        panic!("hatch block")
    };
    let horizontal = |entity: &Entity| {
        let Entity::OnLayer { entity, .. } = entity else {
            panic!("layered")
        };
        let Entity::Line { start, end } = entity.as_ref() else {
            panic!("line")
        };
        start.y == end.y
    };
    let kinds: Vec<_> = block.entities.iter().map(horizontal).collect();
    assert_eq!(kinds, [true, true, true, true, false, false, false, false]);
}

/// The original ends HATCH (`*Invalid*`) on Return at the angle or spacing
/// prompt: END then saves at `Command` with no hatch. Native keeps the
/// prompt for a retry.
#[test]
fn original_return_at_angle_or_spacing_ends_hatch_where_native_retries() {
    for (name, answers) in [
        ("UNOANG", &["U", ""][..]),
        ("UNOSPACE", &["U", "0", ""][..]),
    ] {
        let mut inputs = SQUARE.to_vec();
        inputs.push("HATCH");
        inputs.extend_from_slice(answers);
        let Some(original) = original(name, &inputs) else {
            return;
        };
        assert_eq!(original.len(), 4, "{name}: only the boundary");
        let (last, setup) = inputs.split_last().unwrap();
        let mut native = rust(name, setup);
        let prompt = native.prompt().to_owned();
        assert!(native.submit(last).is_err(), "{name}");
        assert_eq!(native.prompt(), prompt, "{name}");
    }
}

fn hatch_rows(items: &[Item]) -> Vec<f64> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Block(block) => Some(block),
            _ => None,
        })
        .flat_map(|block| &block.entities)
        .map(|entity| {
            let Entity::OnLayer { entity, .. } = entity else {
                panic!("layered")
            };
            let Entity::Line { start, .. } = entity.as_ref() else {
                panic!("line")
            };
            start.y
        })
        .collect()
}

/// Zero spacing passes the original's prompt and selection then finishes
/// without any block or INSERT; negative spacing hatches the rows of its
/// magnitude, swept from the centre in the opposite direction. Native
/// rejects both at the prompt.
#[test]
fn original_zero_or_negative_spacing_is_accepted_where_native_rejects() {
    let Some(zero) = original("UZERO", &square_hatch(&["U", "0", "0", "N"])) else {
        return;
    };
    assert_eq!(zero.len(), 4, "UZERO: only the boundary");
    let Some(negative) = original("UNEG", &square_hatch(&["U", "0", "-1", "N"])) else {
        return;
    };
    assert_eq!(hatch_blocks(&negative), 1);
    assert_eq!(hatch_rows(&negative), [3.0, 2.0, 1.0, 4.0]);
    let positive = rust("UPOS", &square_hatch(&["U", "0", "1", "N"]));
    assert_eq!(hatch_rows(&positive.drawing().items), [3.0, 4.0, 2.0, 1.0]);
    for spacing in ["0", "-1"] {
        let mut native = rust("U", &[&SQUARE[..], &["HATCH", "U", "0"]].concat());
        assert!(native.submit(spacing).is_err(), "{spacing}");
        assert_eq!(native.prompt(), "HATCH: spacing between lines");
    }
}

/// The sweep starts at the row whose index is (centre - phase) / spacing
/// truncated toward zero, then runs up and down, for `U` and named patterns.
#[test]
fn original_sweep_starts_at_the_truncated_row() {
    let unit = |pattern: &'static str| {
        vec![
            "LINE", "0,0", "1,0", "1,1", "0,1", "C", "HATCH", pattern, "", "", "W", "0,0", "2,2",
        ]
    };
    let cases: [(&str, Vec<&str>); 9] = [
        ("SU12", square_hatch(&["U", "0", "1.2", "N"])),
        ("SU180A", square_hatch(&["U", "180", "1.6", "N"])),
        ("SU180B", square_hatch(&["U", "180", "1.2", "N"])),
        ("SLINE", square_hatch(&["LINE", "9.6", ""])),
        ("SLINE180", square_hatch(&["LINE", "9.6", "180"])),
        ("SPLAST", unit("PLAST")),
        ("SPLASTI", unit("PLASTI")),
        ("STRANS", unit("TRANS")),
        ("SINSUL", unit("INSUL")),
    ];
    for (name, inputs) in cases {
        let Some(original) = original(name, &inputs) else {
            return;
        };
        assert_same(name, &original, &rust(name, &inputs).drawing().items);
    }
    let Some(original) = original("STRUNC", &square_hatch(&["U", "0", "1.2", "N"])) else {
        return;
    };
    // Centre 3 / 1.2 = 2.5: row 2 (y = 2.4) first, not the rounded row 3.
    let rows = hatch_rows(&original);
    assert!(
        rows.len() == 4
            && rows
                .iter()
                .zip([2.4, 3.6, 4.8, 1.2])
                .all(|(a, b)| close(*a, b)),
        "{rows:?}"
    );
}

/// Any double answer other than Y/YES is `N` in the original; native
/// rejects it and keeps the prompt.
#[test]
fn original_other_double_answers_mean_no_where_native_rejects() {
    let Some(original) = original("UDOUBLEX", &square_hatch(&["U", "0", "1", "X"])) else {
        return;
    };
    let native_no = rust("UDOUBLEX", &square_hatch(&["U", "0", "1", "N"]));
    assert_same("UDOUBLEX", &original, &native_no.drawing().items);
    let mut native = rust(
        "UDOUBLEX",
        &[&SQUARE[..], &["HATCH", "U", "0", "1"]].concat(),
    );
    assert!(native.submit("X").is_err());
    assert_eq!(native.prompt(), "HATCH: double hatch area (Y/N) <N>");
}

/// A copy of the System image whose ACAD.PAT bytes are replaced in place.
struct PatchedDisk(PathBuf);
impl PatchedDisk {
    fn new(source: &Path, label: &str, edits: &[(&[u8], &[u8])]) -> Self {
        let mut image = std::fs::read(source).unwrap();
        for (from, to) in edits {
            assert_eq!(from.len(), to.len());
            let at = image
                .windows(from.len())
                .position(|window| window == *from)
                .expect("retained ACAD.PAT text");
            image[at..at + from.len()].copy_from_slice(to);
        }
        let path = std::env::temp_dir().join(format!(
            "acad-hatch-user-{}-{label}.img",
            std::process::id()
        ));
        std::fs::write(&path, image).unwrap();
        Self(path)
    }
}
impl Drop for PatchedDisk {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn original_reads_pattern_names_and_geometry_from_acad_pat_at_run_time() {
    let Some(disk) = disk() else {
        return;
    };
    let patched = PatchedDisk::new(
        &disk,
        "pat",
        &[
            (
                b"*line,Parallel horizontal lines\r\n0, 0,0, 0,.125",
                b"*line,Parallel horizontal lines\r\n0, 0,0, 0,.250",
            ),
            (b"*net,", b"*zzz,"),
        ],
    );
    // LINE now hatches at 0.25: native LINE at scale 2.
    let original = original_on(&patched.0, "PLINE", &square_hatch(&["LINE", "", ""]));
    let native = rust("PLINE", &square_hatch(&["LINE", "2", ""]));
    assert_same("PLINE", &original, &native.drawing().items);
    // The renamed definition is found under its new name only.
    let original = original_on(&patched.0, "PZZZ", &square_hatch(&["ZZZ", "", ""]));
    let native = rust("PZZZ", &square_hatch(&["NET", "", ""]));
    assert_same("PZZZ", &original, &native.drawing().items);
    let mut inputs = SQUARE.to_vec();
    inputs.extend(["HATCH", "NET"]);
    assert_eq!(original_on(&patched.0, "PNET", &inputs).len(), 4);
    // An unknown name also ends HATCH on the unpatched image.
    inputs[SQUARE.len() + 1] = "FOO";
    assert_eq!(original_on(&disk, "PFOO", &inputs).len(), 4);
}

/// ACAD.PAT patterns outside the native built-in catalogue equal the native
/// external-file route given the retained file.
#[test]
fn original_acad_pat_only_patterns_match_the_native_file_route() {
    let Some(disk) = disk() else {
        return;
    };
    let pat = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/System/ACAD.PAT");
    let Ok(bytes) = std::fs::read(&pat) else {
        eprintln!("skipping: retained corpus/System/ACAD.PAT absent");
        return;
    };
    for (name, pattern) in [
        ("PANSI31", "ANSI31"),
        ("PANSI37", "ANSI37"),
        ("PBRICK", "BRICK"),
    ] {
        let inputs = square_hatch(&[pattern, "", ""]);
        let original = original_on(&disk, name, &inputs);
        assert_eq!(hatch_blocks(&original), 1, "{name}");
        let mut native = rust(name, &[&SQUARE[..], &["HATCH"]].concat());
        assert_eq!(
            native.hatch_pattern_file_request(pattern).as_deref(),
            Some(pattern)
        );
        native
            .submit_hatch_pattern_file(pattern, "ACAD.PAT", &bytes)
            .unwrap();
        for input in &inputs[SQUARE.len() + 2..] {
            native.submit(input).unwrap();
        }
        assert_same(name, &original, &native.drawing().items);
    }
}

/// A continuous row's spans run in ascending x when the row is closer to
/// horizontal and ascending y otherwise (ties by y), whatever the pattern
/// direction; holes split rows into several spans.
#[test]
fn original_continuous_rows_run_in_ascending_x_or_y() {
    let hole = ["LINE", "2,2", "4,2", "4,4", "2,4", "C"];
    let angles = [
        "100", "135", "150", "180", "200", "225", "260", "270", "300", "315", "330",
    ];
    for (index, angle) in angles.into_iter().enumerate() {
        let mut inputs: Vec<&str> = SQUARE.to_vec();
        inputs.extend(hole);
        inputs.extend(["HATCH", "U", angle, "0.7", "Y"]);
        inputs.extend(WINDOW);
        let name = format!("DIR{index}");
        let Some(original) = original(&name, &inputs) else {
            return;
        };
        assert_same(angle, &original, &rust(&name, &inputs).drawing().items);
    }
    let inputs = square_hatch(&["LINE", "5.6", "180"]);
    let Some(original) = original("DIRLINE", &inputs) else {
        return;
    };
    let Item::Block(block) = &original[4] else {
        panic!("hatch block")
    };
    let Entity::OnLayer { entity, .. } = &block.entities[0] else {
        panic!("layered")
    };
    let Entity::Line { start, end } = entity.as_ref() else {
        panic!("line")
    };
    assert!(start.x < end.x, "{start:?} {end:?}");
    assert_same(
        "DIRLINE",
        &original,
        &rust("DIRLINE", &inputs).drawing().items,
    );
}
