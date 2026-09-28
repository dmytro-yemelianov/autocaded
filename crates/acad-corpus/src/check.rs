#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    CorruptAt(usize),
    Binary,
}

/// A text file from the 1.4 disks is intact only if every byte up to its DOS
/// EOF marker is printable ASCII, CR or LF. Bytes after the marker are FAT
/// cluster slack, not content.
pub fn classify(name: &str, bytes: &[u8]) -> Verdict {
    // .MNU is deliberately excluded: menu files embed the literal control
    // byte each item sends (0x02 snap, 0x03 cancel, 0x0f ortho), so the
    // printable-ASCII rule reports them as corrupt when they are intact.
    let textual = name.ends_with(".DXF") || name.ends_with(".DOC") || name.ends_with(".BAT");
    if !textual {
        return Verdict::Binary;
    }

    // One implementation of "what may appear in a 1983 text file", shared with
    // the DXF lexer so the manifest and the parser can never disagree.
    match acad_dxf::first_non_text_byte(bytes) {
        Some(off) => Verdict::CorruptAt(off),
        None => Verdict::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_file_is_ok() {
        let bytes = b"LINE,1\r\n1.0,2.0,3.0,4.0\r\n\x1a".to_vec();
        assert_eq!(classify("SUBDIV.DXF", &bytes), Verdict::Ok);
    }

    #[test]
    fn spliced_binary_is_reported_at_its_offset() {
        let mut bytes = b"LINE,1\r\n1.0,2.0,3.0,4.0\r\n".to_vec();
        bytes.extend_from_slice(&[0x00, 0x88, 0x06, 0x73]);
        assert_eq!(classify("SHUTTLE.DXF", &bytes), Verdict::CorruptAt(25));
    }

    #[test]
    fn slack_after_dos_eof_is_not_corruption() {
        let mut bytes = b"TREE\r\n\x1a".to_vec();
        bytes.extend_from_slice(b"50,1.00000");
        assert_eq!(classify("SUBDIV.DXF", &bytes), Verdict::Ok);
    }

    #[test]
    fn menu_files_embed_control_codes_so_are_not_plain_text() {
        // ACAD.MNU holds `[^Snap]\x02` and `[^Ortho]\x0f`: the literal control
        // byte each menu item sends. Not corruption, so .MNU is not checkable
        // by the printable-ASCII rule.
        let bytes = b"[< GO >];\r\n[^Snap]\x02\r\n".to_vec();
        assert_eq!(classify("ACAD.MNU", &bytes), Verdict::Binary);
    }

    #[test]
    fn latin1_high_bytes_are_text_not_corruption() {
        // 0xB0 is the degree sign in Latin-1; an intact drawing may contain it.
        let mut bytes = b"TEXT,1\r\n1.0,2.0,0.2,0.0\r\n".to_vec();
        bytes.extend_from_slice(&[0xB0]);
        bytes.extend_from_slice(b"C\r\n\x1a");
        assert_eq!(classify("SUBDIV.DXF", &bytes), Verdict::Ok);
    }

    #[test]
    fn non_text_files_are_classified_binary_not_corrupt() {
        assert_eq!(classify("ACAD.EXE", &[0x4d, 0x5a, 0x00]), Verdict::Binary);
    }
}
