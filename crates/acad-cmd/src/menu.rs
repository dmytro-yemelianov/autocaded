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
    /// A leading `*` on an unlabelled command repeats the command until cancel.
    pub repeat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEntryKind {
    Item,
    Header,
}

pub fn parse_menu(bytes: &[u8]) -> Result<MenuFile, String> {
    let mut entries = Vec::new();
    for (line_number, raw) in bytes.split(|byte| *byte == b'\n').enumerate() {
        let line = raw.strip_suffix(b"\r").unwrap_or(raw);
        if line.is_empty() || line == [0x1a] {
            continue;
        }
        if line.contains(&0x1a) {
            break;
        }

        let (label, action, kind, repeat) = if line.first() == Some(&b'[') {
            let close = line
                .iter()
                .position(|byte| *byte == b']')
                .ok_or_else(|| format!("menu line {}: missing closing `]`", line_number + 1))?;
            let label = printable_label(&line[1..close], line_number + 1)?;
            let tail = &line[close + 1..];
            if tail == b";" {
                (label, tail.to_vec(), MenuEntryKind::Header, false)
            } else {
                (label, tail.to_vec(), MenuEntryKind::Item, false)
            }
        } else {
            let repeat = line.first() == Some(&b'*');
            let action = if repeat { &line[1..] } else { line };
            let label = printable_label(action, line_number + 1)?;
            (label, action.to_vec(), MenuEntryKind::Item, repeat)
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
    if entries.is_empty() {
        return Err("menu file contains no entries".into());
    }
    Ok(MenuFile { entries })
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

    #[test]
    fn acad_menu_keeps_control_bytes_and_macro_text_exactly() {
        let menu = parse_menu(include_bytes!("../../../corpus/System/ACAD.MNU")).unwrap();
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
}
