use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReError {
    /// The file is shorter than the 211-byte header `ACAD.EXE` reads.
    ShortHeader { len: usize },
    /// The first eight bytes are not `AC1.40\0\0`.
    BadMagic { found: [u8; 8] },
    /// A directory entry points past the end of the file.
    RegionPastEof {
        entry: usize,
        region: &'static str,
        end: u64,
        file_len: u64,
    },
    /// A region reaches past the end of the window it pages into.
    RegionPastWindow {
        entry: usize,
        dest: u16,
        end: u32,
        window: u16,
    },
}

impl fmt::Display for ReError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortHeader { len } => {
                write!(
                    f,
                    "overlay file is {len} bytes: shorter than the 211-byte header"
                )
            }
            Self::BadMagic { found } => {
                write!(
                    f,
                    "overlay magic is {:?}, expected \"AC1.40\"",
                    String::from_utf8_lossy(found)
                )
            }
            Self::RegionPastEof {
                entry,
                region,
                end,
                file_len,
            } => write!(
                f,
                "entry {entry} {region} region ends at {end:#x}, past end of file {file_len:#x}"
            ),
            Self::RegionPastWindow {
                entry,
                dest,
                end,
                window,
            } => write!(
                f,
                "entry {entry} code region at {dest:#x} ends at {end:#x}, past the {window:#x}-byte window"
            ),
        }
    }
}

impl std::error::Error for ReError {}
