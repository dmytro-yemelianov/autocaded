use crate::ReError;
use serde::{Deserialize, Serialize};

/// `ACAD.EXE` freads exactly this many bytes and strcmps the magic.
pub const HEADER_LEN: usize = 211;
/// The directory begins here; `ACAD.EXE`'s chain loop starts at `&header[13]`.
pub const DIR_OFF: usize = 13;
/// `{u16 dest; u16 len; u32 file_off}` twice, then `u16 entry_point`.
pub const ENTRY_LEN: usize = 18;
/// `(211 - 13) / 18`.
pub const ENTRY_COUNT: usize = (HEADER_LEN - DIR_OFF) / ENTRY_LEN;

/// One past the last byte an overlay data region may occupy.
///
/// Overlay data pages into `ACAD.EXE`'s own data segment at `DS:0x0002-0x3552`;
/// `DS:0x3553` is the first byte of the EXE's string pool. Unlike the code
/// window, whose size the overlay file declares at `+11`, this bound is a
/// property of `ACAD.EXE` (spec §4.1) and so is a constant here.
pub const DATA_WINDOW_END: u16 = 0x3553;

/// One contiguous run of file bytes paged into one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// Byte offset within the destination window.
    pub dest: u16,
    pub len: u16,
    pub file_off: u32,
}

impl Region {
    pub fn end(&self) -> u64 {
        self.file_off as u64 + self.len as u64
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// One overlay: a code region, a data region, and where to call once loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub index: usize,
    pub code: Region,
    pub data: Region,
    pub entry_point: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Directory {
    /// Bytes `ACAD.EXE` reserves for the code window, from header `+11`.
    pub window_bytes: u16,
    pub entries: Vec<Entry>,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn region_at(b: &[u8], at: usize) -> Region {
    Region {
        dest: u16_at(b, at),
        len: u16_at(b, at + 2),
        file_off: u16_at(b, at + 4) as u32 | (u16_at(b, at + 6) as u32) << 16,
    }
}

/// Decode the 211-byte header `ACAD.EXE` reads from the front of `ACAD.OVL`.
pub fn parse(bytes: &[u8]) -> Result<Directory, ReError> {
    if bytes.len() < HEADER_LEN {
        return Err(ReError::ShortHeader { len: bytes.len() });
    }
    let mut magic = [0u8; 8];
    magic.copy_from_slice(&bytes[..8]);
    if &magic[..6] != b"AC1.40" {
        return Err(ReError::BadMagic { found: magic });
    }

    let file_len = bytes.len() as u64;
    let window = u16_at(bytes, 11);
    let mut entries = Vec::with_capacity(ENTRY_COUNT);
    for index in 0..ENTRY_COUNT {
        let at = DIR_OFF + index * ENTRY_LEN;
        let entry = Entry {
            index,
            code: region_at(bytes, at),
            data: region_at(bytes, at + 8),
            entry_point: u16_at(bytes, at + 16),
        };
        for (region, name) in [(entry.code, "code"), (entry.data, "data")] {
            if !region.is_empty() && region.end() > file_len {
                return Err(ReError::RegionPastEof {
                    entry: index,
                    region: name,
                    end: region.end(),
                    file_len,
                });
            }
        }
        // Each region is read straight into its window, so a region ending past
        // that window's edge writes off the end of it — the silent clobber that
        // matters here. Code and data cannot collide with *each other*: region 1
        // always pages into the code window and region 2 into the EXE's own data
        // segment, so their offsets overlap by design. Computed in u32 because
        // dest + len overflows u16 near the top of a window.
        for (region, name, bound) in [
            (entry.code, "code", window),
            (entry.data, "data", DATA_WINDOW_END),
        ] {
            let end = region.dest as u32 + region.len as u32;
            if !region.is_empty() && end > bound as u32 {
                return Err(ReError::RegionPastWindow {
                    entry: index,
                    region: name,
                    dest: region.dest,
                    end,
                    window: bound,
                });
            }
        }

        entries.push(entry);
    }

    Ok(Directory {
        window_bytes: window,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic OVL header: magic, window size, then `entries`
    /// packed at +13, padded out to the 211-byte header length.
    fn header(window: u16, entries: &[[u16; 9]]) -> Vec<u8> {
        let mut h = vec![0u8; 211];
        h[..6].copy_from_slice(b"AC1.40");
        h[11..13].copy_from_slice(&window.to_le_bytes());
        for (i, e) in entries.iter().enumerate() {
            let at = 13 + i * 18;
            for (j, w) in e.iter().enumerate() {
                h[at + j * 2..at + j * 2 + 2].copy_from_slice(&w.to_le_bytes());
            }
        }
        h
    }

    /// A body long enough that every region in `header` is in range.
    fn with_body(mut h: Vec<u8>, len: usize) -> Vec<u8> {
        h.resize(len, 0);
        h
    }

    #[test]
    fn parses_window_size_and_eleven_entries() {
        let bytes = with_body(header(0xfd00, &[]), 0x2bd18);
        let dir = parse(&bytes).unwrap();
        assert_eq!(dir.window_bytes, 0xfd00);
        assert_eq!(
            dir.entries.len(),
            11,
            "the directory is always 198/18 entries"
        );
    }

    #[test]
    fn entry_fields_decode_in_file_order() {
        // dest,len,off_lo,off_hi for code; then the same for data; then entry point.
        let e = [
            0x0100, 0x5ca0, 0x0100, 0x0000, 0x0002, 0x351e, 0x5e00, 0x0000, 0x0100,
        ];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        let dir = parse(&bytes).unwrap();
        let first = &dir.entries[0];
        assert_eq!(first.index, 0);
        assert_eq!(
            first.code,
            Region {
                dest: 0x0100,
                len: 0x5ca0,
                file_off: 0x0100
            }
        );
        assert_eq!(
            first.data,
            Region {
                dest: 0x0002,
                len: 0x351e,
                file_off: 0x5e00
            }
        );
        assert_eq!(first.entry_point, 0x0100);
    }

    #[test]
    fn high_word_of_the_file_offset_is_honoured() {
        // Region 1 at file offset 0x0001_d600 — above 64 KiB, so the high word matters.
        let e = [0x3770, 0xc590, 0xd600, 0x0001, 0, 0, 0, 0, 0x4d90];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert_eq!(parse(&bytes).unwrap().entries[0].code.file_off, 0x0001_d600);
    }

    #[test]
    fn wrong_magic_is_a_named_error() {
        let mut bytes = with_body(header(0xfd00, &[]), 0x2bd18);
        bytes[..6].copy_from_slice(b"AC1.20");
        assert_eq!(
            parse(&bytes).unwrap_err(),
            ReError::BadMagic {
                found: *b"AC1.20\0\0"
            }
        );
    }

    #[test]
    fn a_file_shorter_than_the_header_is_an_error_not_a_panic() {
        let bytes = header(0xfd00, &[])[..200].to_vec();
        assert_eq!(
            parse(&bytes).unwrap_err(),
            ReError::ShortHeader { len: 200 }
        );
    }

    #[test]
    fn a_region_running_past_end_of_file_names_the_entry() {
        // len 0x100 at offset 0x2bd00 in a 0x2bd18-byte file overruns by 0xe8.
        let e = [0x0100, 0x0100, 0xbd00, 0x0002, 0, 0, 0, 0, 0x0100];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert_eq!(
            parse(&bytes).unwrap_err(),
            ReError::RegionPastEof {
                entry: 0,
                region: "code",
                end: 0x2be00,
                file_len: 0x2bd18
            }
        );
    }

    #[test]
    fn an_empty_region_is_not_an_overrun() {
        // Entries 0..10 are zero-filled by `header`, so every region is len 0 at
        // offset 0. A zero-length region is absent, not a read past the start.
        let bytes = with_body(header(0xfd00, &[]), 211);
        let dir = parse(&bytes).unwrap();
        assert!(dir
            .entries
            .iter()
            .all(|e| e.code.is_empty() && e.data.is_empty()));
    }

    #[test]
    fn a_code_region_overrunning_the_declared_window_is_an_error() {
        // ACAD.EXE reserves exactly `window_bytes` for the code window. A region
        // reaching past it would have the loader write off the end of the
        // allocation — the silent clobber Review Focus 4 exists to catch.
        let e = [0xfd00, 0x0100, 0x0100, 0x0000, 0, 0, 0, 0, 0x0100];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert_eq!(
            parse(&bytes).unwrap_err(),
            ReError::RegionPastWindow {
                entry: 0,
                region: "code",
                dest: 0xfd00,
                end: 0xfe00,
                window: 0xfd00
            }
        );
    }

    #[test]
    fn a_code_region_ending_exactly_at_the_window_edge_is_allowed() {
        // Entry 2 of the real overlay does exactly this: 0x3770 + 0xc590 =
        // 0xfd00. An off-by-one here would reject the shipping file.
        let e = [0x3770, 0xc590, 0x0100, 0x0000, 0, 0, 0, 0, 0x4d90];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert!(parse(&bytes).is_ok());
    }

    #[test]
    fn a_data_region_reaching_the_exe_string_pool_is_an_error() {
        // Overlay data pages into the EXE's own data segment at DS:0x0002-0x3552.
        // DS:0x3553 is the first byte of ACAD.EXE's string pool — a data region
        // reaching it would overwrite "Not enough core for overlays". The codec
        // proved this for the shipping file in a corpus test; it must enforce it
        // for every file.
        let e = [0, 0, 0, 0, 0x3500, 0x0100, 0x5e00, 0x0000, 0x0100];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert_eq!(
            parse(&bytes).unwrap_err(),
            ReError::RegionPastWindow {
                entry: 0,
                region: "data",
                dest: 0x3500,
                end: 0x3600,
                window: DATA_WINDOW_END,
            }
        );
    }

    #[test]
    fn a_data_region_ending_exactly_at_the_string_pool_is_allowed() {
        // The bound is the first byte of the pool, so ending on it is legal.
        let e = [0, 0, 0, 0, 0x3453, 0x0100, 0x5e00, 0x0000, 0x0100];
        let bytes = with_body(header(0xfd00, &[e]), 0x2bd18);
        assert!(parse(&bytes).is_ok());
    }

    #[test]
    fn the_overrun_error_names_which_window_was_exceeded() {
        // Code and data page into different windows; an error that did not say
        // which would send a reader to the wrong one.
        let code = [0xfd00, 0x0100, 0x0100, 0x0000, 0, 0, 0, 0, 0x0100];
        let bytes = with_body(header(0xfd00, &[code]), 0x2bd18);
        let ReError::RegionPastWindow { region, .. } = parse(&bytes).unwrap_err() else {
            panic!("expected RegionPastWindow");
        };
        assert_eq!(region, "code");
    }
}
