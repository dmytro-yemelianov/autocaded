//! In-tree original evidence for DIM text placement across a vertical
//! dimension line (docs/native-dim.md). Each script runs on the original
//! ACAD.EXE with the in-tree 8086/DOS runner; the System image is only read.
//!
//! Derived rule (X extensions, horizontal text): the text is centred on the
//! dimension line unless its half-width W/2 exceeds the reach
//! R = min(|first.x - line.x|, |second.x - line.x|) - A; the excess
//! max(0, W/2 - R) pushes the text centre along the extension direction.
//! `parity` cases compare every ordered LINE/SOLID/TEXT record with the native
//! editor at 1e-10; `falsified` shows that the alternatives considered during
//! measurement each contradict at least one original result; `divergence`
//! pins the original's arrow-fit choice that the native editor does not yet
//! reproduce.
#![cfg(unix)]
use acad_model::{Entity, Item, Point};

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
    eprintln!("skipping in-tree DIM oracle: extracted System.img absent, NOT validated");
    None
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-10
}
fn same_point(a: Point, b: Point) -> bool {
    near(a.x, b.x) && near(a.y, b.y)
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
            same_point(*start, *s) && same_point(*end, *e)
        }
        (
            Entity::Solid { p1, p2, p3, p4 },
            Entity::Solid {
                p1: q1,
                p2: q2,
                p3: q3,
                p4: q4,
            },
        ) => {
            same_point(*p1, *q1)
                && same_point(*p2, *q2)
                && same_point(*p3, *q3)
                && same_point(*p4, *q4)
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
        ) => same_point(*origin, *o) && near(*height, *h) && near(*rotation_deg, *r) && value == v,
        _ => false,
    }
}

fn entities(items: &[Item]) -> Vec<&Entity> {
    items
        .iter()
        .map(|item| match item {
            Item::Entity(entity) => entity,
            other => panic!("unexpected record {other:?}"),
        })
        .collect()
}

fn plain(entity: &Entity) -> &Entity {
    match entity {
        Entity::OnLayer { entity, .. } => entity,
        entity => entity,
    }
}

fn original(disk: &std::path::Path, name: &str, inputs: &[&str]) -> Vec<Item> {
    let dwg = acad_oracle::generate_dwg_in_tree(disk, name, inputs)
        .unwrap_or_else(|error| panic!("{name} {inputs:?}: {error}"));
    acad_dwg::parse(&dwg).unwrap().items
}

fn native(name: &str, inputs: &[&str]) -> Vec<Item> {
    let mut editor = acad_cmd::Editor::default();
    for input in inputs {
        editor
            .submit(input)
            .unwrap_or_else(|error| panic!("{name} native {input:?}: {error}"));
    }
    assert_eq!(editor.prompt(), "Command", "{name}");
    editor.drawing().items.clone()
}

/// Texts of a drawing, in order: (origin, rotation, value).
fn texts(items: &[Item]) -> Vec<(Point, f64, String)> {
    entities(items)
        .into_iter()
        .filter_map(|entity| match plain(entity) {
            Entity::Text {
                origin,
                rotation_deg,
                value,
                ..
            } => Some((*origin, *rotation_deg, value.clone())),
            _ => None,
        })
        .collect()
}

fn script(spec: &str) -> Vec<&str> {
    spec.split('|').collect()
}

/// Scripts measured while deriving the rule (rounds in
/// target/agentic-artifacts/d2-round{1,2,3}.log) and later confirmations.
/// Comments name the round (r0 = the unlogged reproduction of the retained
/// DAR025/DAR050/DAR200 scripts) and the property each case discriminates.
const PARITY: &[&str] = &[
    // r0 (retained DAR025/DAR050/DAR200 scripts) and r1 sweep A = .3 .. 1.5
    // at first (1,1), line x5, second (3,2): centred while W/2 <= 2 - A,
    // then x = 3 + A.
    "DIM|A|0.25|DIM|1,1|5,1|3,2|",
    "DIM|A|0.3|DIM|1,1|5,1|3,2|",
    "DIM|A|0.35|DIM|1,1|5,1|3,2|",
    "DIM|A|0.4|DIM|1,1|5,1|3,2|",
    "DIM|A|0.45|DIM|1,1|5,1|3,2|",
    "DIM|A|0.5|DIM|1,1|5,1|3,2|",
    "DIM|A|0.6|DIM|1,1|5,1|3,2|",
    "DIM|A|0.75|DIM|1,1|5,1|3,2|",
    "DIM|A|1|DIM|1,1|5,1|3,2|",
    "DIM|A|1.5|DIM|1,1|5,1|3,2|",
    "DIM|A|2|DIM|1,1|5,1|3,2|",
    // r2: reach below zero (A > reach).
    "DIM|A|3|DIM|1,1|5,1|3,2|",
    // r1: translation in x and y; view zoom does not participate.
    "DIM|A|0.5|DIM|4,1|8,1|6,2|",
    "DIM|A|0.5|DIM|7,1|11,1|9,2|",
    "DIM|A|0.5|DIM|1,5|5,5|3,6|",
    "ZOOM|.25|DIM|A|0.5|DIM|1,1|5,1|3,2|",
    "ZOOM|4|DIM|A|0.5|DIM|1,1|5,1|3,2|",
    // r1 (4, 2, 6, 8) and r2 (7, 5.25): second-origin distance 1, 2, 3 on
    // both sides of the line, and 0.25.
    "DIM|A|0.5|DIM|1,1|5,1|4,2|",
    "DIM|A|0.5|DIM|1,1|5,1|2,2|",
    "DIM|A|0.5|DIM|1,1|5,1|6,2|",
    "DIM|A|0.5|DIM|1,1|5,1|7,2|",
    "DIM|A|0.5|DIM|1,1|5,1|8,2|",
    "DIM|A|0.5|DIM|1,1|5,1|5.25,2|",
    // r1 (0, 2), r2 (4 with S = 1), r3 (3.5, 4): the shorter extension decides.
    "DIM|A|0.5|DIM|0,1|5,1|3,2|",
    "DIM|A|0.5|DIM|2,1|5,1|3,2|",
    "DIM|A|0.5|DIM|4,1|5,1|1,2|",
    "DIM|A|0.5|DIM|3.5,1|5,1|4,2|",
    "DIM|A|0.5|DIM|4,1|5,1|4,2|",
    // r2: text below the line (negative offset).
    "DIM|A|0.5|DIM|1,2|5,2|3,1|",
    // r2 (first two), r3 (rest): negative extension direction; the push
    // reverses, so the width enters.
    "DIM|A|0.5|DIM|9,1|5,1|7,2|",
    "DIM|A|0.5|DIM|9,1|5,1|3,2|",
    "DIM|A|0.5|DIM|9,2|5,2|7,1|",
    "DIM|A|2|DIM|9,1|5,1|7,2|",
    "DIM|A|0.5|DIM|9,1|5,1|7,2|88888888",
    "DIM|A|0.5|DIM|9,1|5,1|7,2|1",
    "DIM|A|0.5|DIM|9,1|5,1|7,2|WWWW",
    // r1 (`1`), r2 (`88888888`), r3 (`iiii`): label widths (TXT is the only
    // shape font the in-tree runner mounts).
    "DIM|A|0.5|DIM|1,1|5,1|3,2|1",
    "DIM|A|2|DIM|1,1|5,1|3,2|1",
    "DIM|A|0.5|DIM|1,1|5,1|3,2|88888888",
    "DIM|A|0.75|DIM|1,1|5,1|3.4,2|iiii",
    // r2 (r3: A = 3 and T N N): orientations without the push, Y extensions
    // and rotated text.
    "DIM|A|0.5|DIM|1,1|1,5|2,3|",
    "DIM|A|2|DIM|1,1|1,5|2,3|",
    "DIM|A|3|DIM|1,1|1,5|2,3|",
    "DIM|A|0.5|DIM|1,1|1,5|2,4.5|",
    "DIM|A|0.5|DIM|1,1|1,5|2,4.75|",
    "DIM|A|0.5|DIM|T|Y|N|DIM|1,1|5,1|4.25,2|",
    "DIM|A|0.5|DIM|T|N|N|DIM|1,1|5,1|4.25,2|",
    // r2 (C, B) and r3 (mirrored B): B and C after an external dimension.
    "DIM|A|0.5|DIM|1,1|5,1|3,2||DIM|C|3,3|",
    "DIM|A|0.5|DIM|1,1|5,1|3,2||DIM|B|3,3|",
    "DIM|A|0.5|DIM|9,1|5,1|7,2||DIM|B|7,3|",
    // r3: internal text keeps the crossing-text x (both fit rules agree).
    "DIM|A|0.5|DIM|1,1|5,1|3,8|",
    // First D2 pass, after the rule was fixed (not in the round logs):
    // retained PBCB's first three dimensions, whose 5.75 B origin was already
    // known from the D1 audit; internal text with reach 0; a mirrored
    // (sign -1) internal B chain at the default arrow size.
    "DIM|A|0.25|DIM|1,1|5,1|3,2||DIM|A|0.5|DIM|B|2,4||DIM|C|4,6|",
    "DIM|A|0.5|DIM|4.5,1|5,1|3,8|",
    "DIM|9,1|5,1|7,2||DIM|B|8,4|",
];

/// Held out (repair pass 1): scripts absent from every measurement log and
/// from the independent review, with the TEXT origins the rule predicts,
/// written before the original ran them
/// (target/agentic-artifacts/d2-repair1-predictions.log).
const HELD_OUT: &[(&str, &[(f64, f64)])] = &[
    // Just below the A = .4375 threshold: W/2 = 1.5 < R = 1.58, centred.
    ("DIM|A|0.42|DIM|1,1|5,1|3,2|", &[(3.5, 3.26)]),
    // S beyond the line, reaches 3.5 and 1.5: x = L - R.
    ("DIM|A|0.45|DIM|2.5,1|6,1|7.5,2|", &[(4.95, 3.35)]),
    // Sign -1 with a 15-character label: x = L - W + R.
    (
        "DIM|A|0.5|DIM|11,1|6,1|9,2|ABCDEFGHIJKLMNO",
        &[(-1.8214285714285712, 3.5)],
    ),
    // Negative coordinates (LIMITS first), sign -1, text below the line.
    (
        "LIMITS|-20,-20|40,40|DIM|A|0.5|DIM|-3,-1|-7,-1|-6,-2|",
        &[(-10.071428571428573, -4.25)],
    ),
    // Sign -1, second origin closer to the line than A (R = -.25).
    (
        "DIM|A|0.75|DIM|9,1|5,1|5.5,2|",
        &[(-0.6071428571428568, 4.25)],
    ),
    // Internal arrows, first reach 1 (R = .6): x = L - R.
    ("DIM|A|0.4|DIM|3,0.5|4,0.5|2,8.5|", &[(3.4, 4.2)]),
    // Sign -1 B chain: R = .6, then R = 2.4 - .6 = 1.8 for the B.
    (
        "DIM|A|0.6|DIM|11,2|7,2|8.2,3||DIM|B|9,4|",
        &[(3.314285714285715, 4.8), (1.342857142857143, 5.8)],
    ),
];

#[test]
fn held_out_original_matches_predicted_crossing_text() {
    for (spec, expected) in HELD_OUT {
        let origins: Vec<Point> = texts(&native("native", &script(spec)))
            .into_iter()
            .map(|(origin, _, _)| origin)
            .collect();
        assert!(
            origins.len() == expected.len()
                && origins
                    .iter()
                    .zip(*expected)
                    .all(|(o, &(x, y))| same_point(*o, Point { x, y })),
            "native {spec}: {origins:?}"
        );
    }
    let Some(disk) = disk() else { return };
    let mut failures = Vec::new();
    for (index, (spec, expected)) in HELD_OUT.iter().enumerate() {
        let inputs = script(spec);
        let original = original(&disk, &format!("DH{index:02}"), &inputs);
        let native = native("native", &inputs);
        let (o, n) = (entities(&original), entities(&native));
        let predicted = texts(&original).len() == expected.len()
            && texts(&original)
                .iter()
                .zip(*expected)
                .all(|((origin, _, _), &(x, y))| same_point(*origin, Point { x, y }));
        if !predicted || o.len() != n.len() || o.iter().zip(&n).any(|(a, b)| !same_entity(a, b)) {
            failures.push(format!("{spec}\n original {o:?}\n native   {n:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn parity_original_crossing_text_matches_native_records() {
    let Some(disk) = disk() else { return };
    let mut failures = Vec::new();
    for (index, spec) in PARITY.iter().enumerate() {
        let name = format!("DA{index:02}");
        let inputs = script(spec);
        let original = original(&disk, &name, &inputs);
        let native = native(&name, &inputs);
        let (o, n) = (entities(&original), entities(&native));
        if o.len() != n.len() || o.iter().zip(&n).any(|(a, b)| !same_entity(a, b)) {
            failures.push(format!("{spec}\n original {o:?}\n native   {n:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The original text x for each `PARITY` case with X extensions and
/// horizontal text, with the inputs that alternative rules depend on.
struct Observation {
    line_x: f64,
    first_x: f64,
    second_x: f64,
    sign: f64,
    arrow: f64,
    width: f64,
    x: f64,
}

/// The derived reach: shorter extension distance to the line, minus A.
fn reach(o: &Observation) -> f64 {
    (o.first_x - o.line_x)
        .abs()
        .min((o.second_x - o.line_x).abs())
        - o.arrow
}

/// A candidate text-origin x rule.
type Rule = dyn Fn(&Observation) -> f64;

/// Ink width of the observed labels in TXT units (cap height 21).
fn ink_units(label: &str) -> f64 {
    match label {
        "1.0000" => 100.0,
        "1" => 5.0,
        "88888888" => 154.0,
        "WWWW" => 92.0,
        other => panic!("no measured width for {other}"),
    }
}

#[test]
fn falsified_alternative_rules_contradict_original_results() {
    let Some(disk) = disk() else { return };
    // (script, first x, line x, second x, sign, A)
    let cases: &[(&str, f64, f64, f64, f64, f64)] = &[
        ("DIM|A|0.5|DIM|1,1|5,1|3,2|", 1.0, 5.0, 3.0, 1.0, 0.5),
        ("DIM|A|2|DIM|1,1|5,1|3,2|", 1.0, 5.0, 3.0, 1.0, 2.0),
        ("DIM|A|0.5|DIM|4,1|5,1|1,2|", 4.0, 5.0, 1.0, 1.0, 0.5),
        ("DIM|A|0.5|DIM|1,1|5,1|6,2|", 1.0, 5.0, 6.0, 1.0, 0.5),
        ("DIM|A|0.5|DIM|1,1|5,1|5.25,2|", 1.0, 5.0, 5.25, 1.0, 0.5),
        ("DIM|A|0.5|DIM|9,1|5,1|7,2|", 9.0, 5.0, 7.0, -1.0, 0.5),
        (
            "DIM|A|0.5|DIM|9,1|5,1|7,2|88888888",
            9.0,
            5.0,
            7.0,
            -1.0,
            0.5,
        ),
        ("DIM|A|0.5|DIM|9,1|5,1|7,2|WWWW", 9.0, 5.0, 7.0, -1.0, 0.5),
        ("DIM|A|0.5|DIM|9,1|5,1|7,2|1", 9.0, 5.0, 7.0, -1.0, 0.5),
    ];
    let observations: Vec<Observation> = cases
        .iter()
        .enumerate()
        .map(|(index, &(spec, first_x, line_x, second_x, sign, arrow))| {
            let items = original(&disk, &format!("DF{index:02}"), &script(spec));
            let [(origin, rotation, value)] = &texts(&items)[..] else {
                panic!("{spec}: one TEXT expected")
            };
            assert_eq!(*rotation, 0.0, "{spec}");
            Observation {
                line_x,
                first_x,
                second_x,
                sign,
                arrow,
                width: ink_units(value) * 1.5 * arrow / 21.0,
                x: origin.x,
            }
        })
        .collect();
    let rules: [(&str, &Rule); 5] = [
        ("centred", &|o| o.line_x - o.width / 2.0),
        ("second-origin reach only", &|o| {
            let r = (o.second_x - o.line_x).abs() - o.arrow;
            o.line_x + o.sign * (o.width / 2.0 - r).max(0.0) - o.width / 2.0
        }),
        ("first-origin reach only", &|o| {
            let r = (o.first_x - o.line_x).abs() - o.arrow;
            o.line_x + o.sign * (o.width / 2.0 - r).max(0.0) - o.width / 2.0
        }),
        ("unsigned push", &|o| {
            o.line_x + (o.width / 2.0 - reach(o)).max(0.0) - o.width / 2.0
        }),
        ("left edge clamped at second origin + A", &|o| {
            (o.line_x - o.width / 2.0).max(o.second_x + o.arrow)
        }),
    ];
    for (name, rule) in rules {
        assert!(
            observations.iter().any(|o| !near(rule(o), o.x)),
            "alternative {name} is not falsified"
        );
    }
    for o in &observations {
        let derived = o.line_x + o.sign * (o.width / 2.0 - reach(o)).max(0.0) - o.width / 2.0;
        assert!(near(derived, o.x), "derived {derived} original {}", o.x);
    }
}

fn internal_arrows(items: &[Item]) -> bool {
    // The first SOLID's tip lies at the dimension line's first end; its base
    // is inside the span for internal arrows (between the two tips).
    let solids: Vec<(Point, Point)> = entities(items)
        .into_iter()
        .filter_map(|entity| match plain(entity) {
            Entity::Solid { p1, p3, .. } => Some((*p1, *p3)),
            _ => None,
        })
        .collect();
    let [(base, tip), (_, other_tip)] = solids[..] else {
        panic!("two arrows expected")
    };
    let along = |p: Point| p.x * (other_tip.x - tip.x) + p.y * (other_tip.y - tip.y);
    along(base) > along(tip)
}

#[test]
fn divergence_original_fits_inside_where_native_width_rule_does_not() {
    // X extensions, default inside/outside horizontal text, A=.5. The native
    // comparator length >= W + 6A puts spans 6 and 6.5 outside; the original keeps
    // the arrows inside and applies the crossing-text x (3.5 = 5 - (2 - A)).
    // The comparator itself is not determined (docs/native-dim.md).
    let Some(disk) = disk() else { return };
    // Round 2 P14 (span 6) and round 3 P10 (span 6.5).
    for (index, (spec, x, y)) in [
        ("DIM|A|0.5|DIM|1,1|5,1|3,7|", 3.5, 3.625),
        ("DIM|A|0.5|DIM|1,1|5,1|3,7.5|", 3.5, 3.875),
    ]
    .into_iter()
    .enumerate()
    {
        let inputs = script(spec);
        let original = original(&disk, &format!("DV{index:02}"), &inputs);
        assert!(internal_arrows(&original), "{spec}: original internal");
        let [(origin, rotation, _)] = &texts(&original)[..] else {
            panic!("{spec}")
        };
        assert!(
            same_point(*origin, Point { x, y }) && *rotation == 0.0,
            "{spec} {origin:?}"
        );
        assert!(
            !internal_arrows(&native("native", &inputs)),
            "{spec}: native"
        );
    }
    // Retained PBCB: span 3 external, span 4 internal on the same vertical
    // line history at A=.5 (both under h + 6A = 3.75 vs W + 6A > 6.7).
    let pbcb = "DIM|A|0.25|DIM|1,1|5,1|3,2||DIM|A|0.5|DIM|B|2,4||DIM|C|4,6||DIM|B|6,8|";
    let items = original(&disk, "DVPBCB", &script(pbcb));
    let all = entities(&items);
    assert_eq!(all.len(), 28);
    assert!(!internal_arrows(&items[7..14]), "PBCB B span 3 external");
    assert!(
        internal_arrows(&items[21..28]),
        "PBCB final B span 4 internal"
    );
    assert!(!internal_arrows(&native("native", &script(pbcb))[21..28]));
    let (origin, _, value) = &texts(&items)[3];
    assert_eq!(value, "4.0000");
    assert!(
        same_point(*origin, Point { x: 11.0, y: 5.625 }),
        "{origin:?}"
    );
}
