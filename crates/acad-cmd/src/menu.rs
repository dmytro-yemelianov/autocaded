//! Parser for the screen-menu source format used by AutoCAD 1.4 `.MNU` files.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuFile {
    pub entries: Vec<MenuEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub label: String,
    /// The exact bytes sent to the command input stream when this item is used.
    /// Control characters and macro punctuation are intentionally preserved.
    pub action: Vec<u8>,
    pub kind: MenuEntryKind,
    /// A leading `*` (before a label or an unlabelled command) starts a new
    /// screen-menu page.
    pub repeat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEntryKind {
    Item,
    Header,
    /// An empty source line: an inert slot that still occupies a menu row
    /// (docs/native-files-menu.md).
    Blank,
}

pub fn parse_menu(bytes: &[u8]) -> Result<MenuFile, String> {
    let mut entries = Vec::new();
    let mut lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
    // A final line break does not open another (blank) line.
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    for (line_number, raw) in lines.into_iter().enumerate() {
        let line = raw.strip_suffix(b"\r").unwrap_or(raw);
        if line.first() == Some(&0x1a) || line.contains(&0x1a) {
            break;
        }
        if line.is_empty() {
            entries.push(MenuEntry {
                label: String::new(),
                action: Vec::new(),
                kind: MenuEntryKind::Blank,
                repeat: false,
            });
            continue;
        }

        let repeat = line.first() == Some(&b'*');
        let body = if repeat { &line[1..] } else { line };
        let (label, action, kind) = if body.first() == Some(&b'[') {
            let close = body
                .iter()
                .position(|byte| *byte == b']')
                .ok_or_else(|| format!("menu line {}: missing closing `]`", line_number + 1))?;
            let label = printable_label(&body[1..close], line_number + 1)?;
            let tail = &body[close + 1..];
            let kind = if tail == b";" && !repeat {
                MenuEntryKind::Header
            } else {
                MenuEntryKind::Item
            };
            (label, tail.to_vec(), kind)
        } else {
            let label = printable_label(body, line_number + 1)?;
            (label, body.to_vec(), MenuEntryKind::Item)
        };

        if label.is_empty() {
            return Err(format!("menu line {}: empty item label", line_number + 1));
        }
        entries.push(MenuEntry {
            label,
            action,
            kind,
            repeat,
        });
    }
    if entries
        .iter()
        .all(|entry| entry.kind == MenuEntryKind::Blank)
    {
        return Err("menu file contains no entries".into());
    }
    Ok(MenuFile { entries })
}

/// One step of a picked screen-menu macro (docs/native-files-menu.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroStep {
    /// Submit this text with one Return (it may be empty).
    Return(String),
    /// Wait for one user input; the text is already typed before it.
    Pause(String),
}

/// Tokenise macro text: every space and `;` is one Return, `\` pauses for
/// one user input, and pending text at the end is submitted with no extra
/// Return. Measured by `crates/acad-oracle/tests/menu_macros.rs`.
pub fn macro_steps(text: &str) -> Vec<MacroStep> {
    let mut steps = Vec::new();
    let mut pending = String::new();
    for character in text.chars() {
        match character {
            ' ' | ';' => steps.push(MacroStep::Return(std::mem::take(&mut pending))),
            '\\' => steps.push(MacroStep::Pause(std::mem::take(&mut pending))),
            other => pending.push(other),
        }
    }
    if !pending.is_empty() {
        steps.push(MacroStep::Return(pending));
    }
    steps
}

fn printable_label(bytes: &[u8], line: usize) -> Result<String, String> {
    if !bytes.iter().all(|byte| (0x20..=0x7e).contains(byte)) {
        return Err(format!(
            "menu line {line}: label contains a non-printable byte"
        ));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| format!("menu line {line}: invalid label"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A retained corpus file read at run time (the corpus is not in git). Absent
    /// files skip the calling test visibly; `AUTOCAD_REQUIRE_CORPUS=1` makes
    /// absence a failure.
    fn corpus_file(path: &str) -> Option<Vec<u8>> {
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus")
            .join(path);
        match std::fs::read(&full) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                let message = format!("corpus file {} absent: {error}", full.display());
                assert!(
                    std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
                    "{message}"
                );
                eprintln!("skipping corpus test, NOT validated: {message}");
                None
            }
        }
    }

    #[test]
    fn acad_menu_keeps_control_bytes_and_macro_text_exactly() {
        let Some(bytes) = corpus_file("System/ACAD.MNU") else {
            return;
        };
        let menu = parse_menu(&bytes).unwrap();
        let snap = menu
            .entries
            .iter()
            .find(|entry| entry.label == "^Snap")
            .unwrap();
        assert_eq!(snap.action, [0x02]);
        let ortho = menu
            .entries
            .iter()
            .find(|entry| entry.label == "^Ortho")
            .unwrap();
        assert_eq!(ortho.action, [0x0f]);
        let cancel = menu
            .entries
            .iter()
            .find(|entry| entry.label == "^Cancel")
            .unwrap();
        assert_eq!(cancel.action, [0x03]);
        let zoom = menu
            .entries
            .iter()
            .find(|entry| entry.label == "ZOOM All")
            .unwrap();
        assert_eq!(zoom.action, b"zoom a");
        let point = menu
            .entries
            .iter()
            .find(|entry| entry.label == "POINT")
            .unwrap();
        assert_eq!(point.action, b"POINT");
        assert!(point.repeat);
        let header = &menu.entries[0];
        assert_eq!(header.kind, MenuEntryKind::Header);
        assert_eq!(header.action, b";");
        assert!(!header.repeat);
    }

    #[test]
    fn malformed_labels_are_rejected_with_a_line_number() {
        assert_eq!(
            parse_menu(b"[missing\r\n"),
            Err("menu line 1: missing closing `]`".into())
        );
        assert!(parse_menu(b"[x\x03]go\r\n")
            .unwrap_err()
            .contains("non-printable"));
    }

    #[test]
    fn blank_lines_are_inert_slots_and_star_bracket_items_keep_label_and_macro() {
        assert!(parse_menu(b"*[x\r\n").unwrap_err().contains("line 1"));
        assert!(parse_menu(b"\r\n\r\n\x1a")
            .unwrap_err()
            .contains("no entries"));
        let (Some(subdiv), Some(office), Some(shuttle)) = (
            corpus_file("Samples/SUBDIV.MNU"),
            corpus_file("Samples/OFFICE.MNU"),
            corpus_file("Samples/SHUTTLE.MNU"),
        ) else {
            return;
        };
        let menu = parse_menu(&subdiv).unwrap();
        let labels: Vec<&str> = menu.entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels[..9],
            ["A", "B", "C", "D", "TREE", "HYDRANT", "TEXT", "", "WINDOW"]
        );
        let blank = &menu.entries[7];
        assert_eq!(blank.kind, MenuEntryKind::Blank);
        assert!(blank.action.is_empty() && !blank.repeat);
        let shift = &menu.entries[12];
        assert_eq!(shift.label, "SHIFT");
        assert_eq!(shift.action, b"change \\ \\;");
        assert!(shift.repeat);
        assert_eq!(shift.kind, MenuEntryKind::Item);
        // Every retained menu parses; OFFICE.MNU's `*[QUIT]` keeps its label.
        let office = parse_menu(&office).unwrap();
        let quit = office.entries.last().unwrap();
        assert_eq!((quit.label.as_str(), quit.repeat), ("QUIT", true));
        assert_eq!(quit.action, b"quit y;");
        parse_menu(&shuttle).unwrap();
    }

    fn returns(values: &[&str]) -> Vec<MacroStep> {
        values
            .iter()
            .map(|v| MacroStep::Return((*v).into()))
            .collect()
    }

    #[test]
    fn every_space_and_semicolon_is_one_return_and_item_end_adds_none() {
        assert_eq!(macro_steps("zoom a"), returns(&["zoom", "a"]));
        assert_eq!(macro_steps("end;"), returns(&["end"]));
        assert_eq!(macro_steps(";"), returns(&[""]));
        assert_eq!(macro_steps("quit y;"), returns(&["quit", "y"]));
        assert_eq!(
            macro_steps("line 2,2  3,3;"),
            returns(&["line", "2,2", "", "3,3"])
        );
        assert_eq!(
            macro_steps("line 2,2 3,3 "),
            returns(&["line", "2,2", "3,3"])
        );
        assert_eq!(macro_steps(""), returns(&[]));
    }

    #[test]
    fn backslash_pauses_keep_text_before_them_as_the_users_input_prefix() {
        use MacroStep::{Pause, Return};
        assert_eq!(
            macro_steps("insert housea \\1 1 \\"),
            [
                Return("insert".into()),
                Return("housea".into()),
                Pause(String::new()),
                Return("1".into()),
                Return("1".into()),
                Pause(String::new()),
            ]
        );
        assert_eq!(
            macro_steps("change \\ \\;"),
            [
                Return("change".into()),
                Pause(String::new()),
                Return(String::new()),
                Pause(String::new()),
                Return(String::new()),
            ]
        );
        assert_eq!(
            macro_steps("zoom w \\\\"),
            [
                Return("zoom".into()),
                Return("w".into()),
                Pause(String::new()),
                Pause(String::new()),
            ]
        );
        assert_eq!(
            macro_steps("line 1,\\7,7;"),
            [
                Return("line".into()),
                Pause("1,".into()),
                Return("7,7".into()),
            ]
        );
    }
}
