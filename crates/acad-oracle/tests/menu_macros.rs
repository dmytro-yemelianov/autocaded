//! In-tree original evidence for screen-menu macro grammar
//! (docs/native-files-menu.md). The original selects screen-menu items from
//! the keyboard: INS enters the menu cursor, cursor-down moves it and Return
//! picks the highlighted item. Each case loads a private `M1.MNU` into a new
//! drawing, picks an item, types any pause input, cancels and ENDs; the
//! saved drawing is the observation. Every original behaviour the doc cites
//! is asserted here; the System image is only read.
#![cfg(unix)]
use acad_model::{Entity, Point};
use acad_oracle::in_tree::observe_in_tree;

const INS: &[u8] = b"\0\x52";
const DOWN: &[u8] = b"\0\x50";
const PICK: &[u8] = b"\r";

fn disk() -> Option<std::path::PathBuf> {
    let disk = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)/System.img");
    if disk.exists() {
        Some(disk)
    } else {
        eprintln!("skipping in-tree oracle: extracted System.img absent");
        None
    }
}

/// Lines (bare, not erased) saved after `keys` against menu `menu`.
fn saved(disk: &std::path::Path, menu: &[u8], keys: &[&[u8]]) -> Vec<Entity> {
    let mut input = b"1\rD2\rMENU\rM1\r".to_vec();
    for part in keys {
        input.extend_from_slice(part);
    }
    input.extend_from_slice(b"\x03\x03END\r");
    let observation = observe_in_tree(
        disk,
        b"",
        &[("M1.MNU", menu)],
        &[(&input, 400)],
        100_000,
        &["D2.DWG"],
    )
    .unwrap();
    assert_eq!(observation.stopped, None);
    let (_, bytes) = observation.created.first().expect("END saved D2.DWG");
    acad_dwg::parse(bytes)
        .unwrap()
        .entities()
        .map(|mut entity| {
            while let Entity::OnLayer { entity: inner, .. } = entity {
                entity = inner;
            }
            entity.clone()
        })
        .collect()
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    Entity::Line {
        start: Point { x: x0, y: y0 },
        end: Point { x: x1, y: y1 },
    }
}

#[test]
fn keyboard_menu_cursor_picks_items_and_a_blank_line_is_a_slot() {
    let Some(disk) = disk() else { return };
    let menu = b"[A]line 2,2 3,3;\r\n[B]line 4,4 5,5;\r\n\x1a";
    assert_eq!(saved(&disk, menu, &[]), []);
    assert_eq!(saved(&disk, menu, &[INS, PICK]), [line(2.0, 2.0, 3.0, 3.0)]);
    assert_eq!(
        saved(&disk, menu, &[INS, DOWN, PICK]),
        [line(4.0, 4.0, 5.0, 5.0)]
    );
    // An empty source line occupies a menu slot: B is two rows below A.
    let blank = b"[A]line 2,2 3,3;\r\n\r\n[B]line 4,4 5,5;\r\n\x1a";
    assert_eq!(
        saved(&disk, blank, &[INS, DOWN, DOWN, PICK]),
        [line(4.0, 4.0, 5.0, 5.0)]
    );
}

#[test]
fn a_leading_star_before_a_bracket_label_keeps_the_macro() {
    let Some(disk) = disk() else { return };
    assert_eq!(
        saved(&disk, b"*[B]line 4,4 5,5;\r\n\x1a", &[INS, PICK]),
        [line(4.0, 4.0, 5.0, 5.0)]
    );
    // `*` also starts a new page: the same B without `*` is one row below A
    // (keyboard_menu_cursor_picks_items...), but with `*` that row is empty.
    assert_eq!(
        saved(
            &disk,
            b"[A]line 2,2 3,3;\r\n*[B]line 4,4 5,5;\r\n\x1a",
            &[INS, DOWN, PICK]
        ),
        []
    );
}

#[test]
fn every_space_and_semicolon_is_one_return() {
    let Some(disk) = disk() else { return };
    // The second space is an empty Return that ends LINE before 3,3.
    assert_eq!(saved(&disk, b"[A]line 2,2  3,3;\r\n\x1a", &[INS, PICK]), []);
}

#[test]
fn item_end_submits_pending_text_but_adds_no_extra_return() {
    let Some(disk) = disk() else { return };
    // Pending `3,3` is submitted at the end of the item; LINE stays open.
    // A trailing space or semicolon is the only Return: LINE stays open.
    for menu in [
        &b"[A]line 2,2 3,3\r\n\x1a"[..],
        b"[A]line 2,2 3,3 \r\n\x1a",
        b"[A]line 2,2 3,3;\r\n\x1a",
    ] {
        assert_eq!(
            saved(&disk, menu, &[INS, PICK, b"4,4\r"]),
            [line(2.0, 2.0, 3.0, 3.0), line(3.0, 3.0, 4.0, 4.0)],
            "{}",
            String::from_utf8_lossy(menu)
        );
    }
}

#[test]
fn backslash_pauses_for_one_user_input_then_continues() {
    let Some(disk) = disk() else { return };
    assert_eq!(
        saved(&disk, b"[A]line 1,1 \\;\r\n\x1a", &[INS, PICK, b"7,7\r"]),
        [line(1.0, 1.0, 7.0, 7.0)]
    );
    // Consecutive pauses each take one input.
    assert_eq!(
        saved(&disk, b"[A]line \\\\;\r\n\x1a", &[INS, PICK, b"1,1\r2,2\r"]),
        [line(1.0, 1.0, 2.0, 2.0)]
    );
    // Text right after a pause starts the next input.
    assert_eq!(
        saved(&disk, b"[A]line \\3,3;\r\n\x1a", &[INS, PICK, b"1,1\r"]),
        [line(1.0, 1.0, 3.0, 3.0)]
    );
    // Text right before a pause stays as the start of the user's input.
    assert_eq!(
        saved(&disk, b"[A]line 1,\\7,7;\r\n\x1a", &[INS, PICK, b"1\r"]),
        [line(1.0, 1.0, 7.0, 7.0)]
    );
    // A pause at item end adds no Return after the user's input.
    assert_eq!(
        saved(
            &disk,
            b"[A]line 1,1 \\\r\n\x1a",
            &[INS, PICK, b"2,2\r", b"3,3\r\r"]
        ),
        [line(1.0, 1.0, 2.0, 2.0), line(2.0, 2.0, 3.0, 3.0)]
    );
}

#[test]
fn a_pause_satisfies_object_selection_like_the_retained_erase_macro() {
    let Some(disk) = disk() else { return };
    let draw: &[u8] = b"LINE\r0,0\r1,1\r\r";
    assert_eq!(
        saved(&disk, b"[E]x\r\n\x1a", &[draw]),
        [line(0.0, 0.0, 1.0, 1.0)]
    );
    // SUBDIV.MNU / OFFICE.MNU `[ERASE 1]erase \;`.
    assert_eq!(
        saved(&disk, b"[E]erase \\;\r\n\x1a", &[draw, INS, PICK, b"L\r"]),
        []
    );
}

#[test]
fn cancel_during_a_pause_abandons_the_rest_of_the_macro() {
    let Some(disk) = disk() else { return };
    assert_eq!(
        saved(
            &disk,
            b"[A]line 1,1 \\9,9;\r\n\x1a",
            &[INS, PICK, b"\x03", b"LINE\r5,5\r6,6\r\r"]
        ),
        [line(5.0, 5.0, 6.0, 6.0)]
    );
}
