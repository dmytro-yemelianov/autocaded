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
    /// Two regions of one entry claim the same bytes of the same window.
    RegionsOverlap {
        entry: usize,
        window: &'static str,
        at: u16,
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
            Self::RegionsOverlap { entry, window, at } => {
                write!(
                    f,
                    "entry {entry}: code and data overlap in the {window} window at {at:#x}"
                )
            }
        }
    }
}

impl std::error::Error for ReError {}
