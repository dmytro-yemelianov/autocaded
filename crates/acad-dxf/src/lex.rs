use crate::error::DxfError;

/// One instance of one record. For entity headers, the number after the
/// comma is the layer, so `LINE,20` yields one `Record`.
/// `line` is the 1-based index of the record's header line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub keyword: String,
    pub rows: Vec<String>,
    pub line: usize,
}

const DOS_EOF: u8 = 0x1a;

/// Rows consumed by one instance of a record. `POINT`, `TRACE`, `SOLID`,
/// `REPEAT` and `ENDREP` are evidenced by DXFs exported by the original
/// from the AC1.2 corpus; `LOAD` and `SHAPE` by generated AC1.40 drawings.
pub fn rows_per_instance(keyword: &str) -> Option<usize> {
    Some(match keyword {
        "ENDBLK" | "REPEAT" => 0,
        "LAYERC" => 8,
        "TEXT" | "INSERT" | "BLOCK" | "TRACE" | "SOLID" => 2,
        "EXTENTS" | "LIMITS" | "BASE" | "DWGVIEW" | "MODERES" | "MODEGRID" | "MODEORTHO"
        | "MODEFILL" | "TXTSIZE" | "TRACEWID" | "LAYER" | "LINE" | "CIRCLE" | "ARC" | "POINT"
        | "ENDREP" | "LOAD" | "SHAPE" => 1,
        _ => return None,
    })
}

fn is_entity(keyword: &str) -> bool {
    matches!(
        keyword,
        "LINE"
            | "LOAD"
            | "SHAPE"
            | "CIRCLE"
            | "ARC"
            | "POINT"
            | "TRACE"
            | "SOLID"
            | "TEXT"
            | "INSERT"
            | "BLOCK"
            | "ENDBLK"
            | "REPEAT"
            | "ENDREP"
    )
}

/// Offset of the first byte that cannot occur in a 1983 text file, ignoring
/// anything past the DOS EOF marker (that is FAT cluster slack, not content).
///
/// Only C0 control bytes other than CR and LF disqualify a file. Bytes >= 0x80
/// are legal Latin-1 text -- rejecting them reports intact drawings as corrupt.
pub fn first_non_text_byte(bytes: &[u8]) -> Option<usize> {
    let end = bytes
        .iter()
        .position(|&b| b == DOS_EOF)
        .unwrap_or(bytes.len());
    bytes[..end]
        .iter()
        .position(|&b| b < 0x20 && b != b'\r' && b != b'\n')
}

pub fn lex(bytes: &[u8]) -> Result<Vec<Record>, DxfError> {
    if let Some(off) = first_non_text_byte(bytes) {
        return Err(DxfError::Corrupt { offset: off });
    }
    let end = bytes
        .iter()
        .position(|&b| b == DOS_EOF)
        .unwrap_or(bytes.len());
    // Latin-1: `u8 as char` maps 0x00..=0xFF onto U+0000..=U+00FF, which is
    // exactly Latin-1, so this is a faithful decode rather than an ASCII one.
    let text: String = bytes[..end].iter().map(|&b| b as char).collect();
    // Drop only the empty element after the file's final CRLF. Filtering every
    // empty line would swallow a legitimately empty TEXT value and would shift
    // every reported line number away from the real one.
    let mut lines: Vec<&str> = text.split("\r\n").collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }

    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let (kw, count) = lines[i]
            .rsplit_once(',')
            .and_then(|(k, n)| n.parse::<usize>().ok().map(|n| (k.to_string(), n)))
            .ok_or(DxfError::BadHeader { line: i + 1 })?;
        let per = rows_per_instance(&kw).ok_or_else(|| DxfError::UnknownKeyword {
            keyword: kw.clone(),
            line: i + 1,
        })?;
        let mut cursor = i + 1;
        for _ in 0..if is_entity(&kw) { 1 } else { count } {
            if cursor + per > lines.len() {
                return Err(DxfError::Truncated {
                    keyword: kw.clone(),
                    line: i + 1,
                });
            }
            out.push(Record {
                keyword: kw.clone(),
                line: i + 1,
                rows: lines[cursor..cursor + per]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            });
            cursor += per;
        }
        i = cursor;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_a_single_line_record() {
        let src = b"LINE,1\r\n8.800000,5.700000,19.000000,1.899999\r\n\x1a";
        let recs = lex(src).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].keyword, "LINE");
        assert_eq!(recs[0].rows, vec!["8.800000,5.700000,19.000000,1.899999"]);
    }

    #[test]
    fn layerc_takes_eight_rows_for_one_instance() {
        let mut src = b"LAYERC,1\r\n".to_vec();
        for _ in 0..8 {
            src.extend_from_slice(
                b"0,15,255,255,255,255,255,255,255,255,255,255,255,255,255,255\r\n",
            );
        }
        src.push(0x1a);
        let recs = lex(&src).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].rows.len(), 8);
    }

    #[test]
    fn entity_suffix_is_layer_not_instance_count() {
        let src = b"LINE,20\r\n0,0,1,1\r\nPOINT,3\r\n2,2\r\n\x1a";
        let recs = lex(src).unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].rows, vec!["0,0,1,1"]);
        assert_eq!(recs[1].rows, vec!["2,2"]);
    }

    #[test]
    fn text_record_takes_two_rows() {
        let src = b"TEXT,1\r\n9.624130,12.295160,0.346010,0.000000\r\nA\r\n\x1a";
        let recs = lex(src).unwrap();
        assert_eq!(
            recs[0].rows,
            vec!["9.624130,12.295160,0.346010,0.000000", "A"]
        );
    }

    #[test]
    fn endblk_takes_no_rows() {
        let src = b"ENDBLK,1\r\nLINE,1\r\n0,0,1,1\r\n\x1a";
        let recs = lex(src).unwrap();
        assert_eq!(recs.len(), 2);
        assert!(recs[0].rows.is_empty());
        assert_eq!(recs[1].keyword, "LINE");
    }

    #[test]
    fn spliced_binary_is_an_error_naming_the_offset() {
        // "LINE,1"+CRLF = 8, "8.8,5.7,19.0,1.9" = 16, +CRLF = 26 bytes of text.
        let mut src = b"LINE,1\r\n8.8,5.7,19.0,1.9\r\n".to_vec();
        src.extend_from_slice(&[0x00, 0x88, 0x06, 0x73]);
        assert_eq!(lex(&src), Err(DxfError::Corrupt { offset: 26 }));
    }

    #[test]
    fn unimplemented_entity_is_rejected_not_guessed() {
        let src = b"UNKNOWN,1\r\n0,0,1,1\r\n\x1a";
        assert_eq!(
            lex(src),
            Err(DxfError::UnknownKeyword {
                keyword: "UNKNOWN".into(),
                line: 1
            })
        );
    }

    #[test]
    fn record_running_past_end_of_file_is_truncated() {
        let src = b"TEXT,1\r\n9.6,12.2,0.3,0.0\r\n\x1a";
        assert_eq!(
            lex(src),
            Err(DxfError::Truncated {
                keyword: "TEXT".into(),
                line: 1
            })
        );
    }
}
