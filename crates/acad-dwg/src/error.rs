use std::fmt;

use crate::header::Version;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DwgError {
    /// The first bytes are neither `AC1.2` nor `AC1.40`.
    UnknownVersion { found: [u8; 8] },
    /// A version this codec knows of but cannot read yet.
    UnsupportedVersion { found: Version, supported: Version },
    /// The file is shorter than the fixed header.
    ShortHeader { len: usize, need: usize },
    /// An entity record runs past the end of the file.
    TruncatedEntity { index: u32, at: usize, len: usize },
    /// An entity type code this codec does not know.
    UnknownEntityType { code: u16, at: usize },
    /// The header promises more entities than the bytes hold.
    EntityCountMismatch { want: u32, got: u32 },
}

impl fmt::Display for DwgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownVersion { found } => write!(
                f,
                "not a DWG: magic is {:?}, expected \"AC1.2\" or \"AC1.40\"",
                String::from_utf8_lossy(found)
            ),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "this is a {found} drawing; only {supported} can be read so far"
            ),
            Self::ShortHeader { len, need } => {
                write!(f, "file is {len} bytes: the header alone needs {need}")
            }
            Self::TruncatedEntity { index, at, len } => write!(
                f,
                "entity {index} at offset {at:#x} runs past end of file {len:#x}"
            ),
            Self::UnknownEntityType { code, at } => {
                write!(f, "unknown entity type {code:#x} at offset {at:#x}")
            }
            Self::EntityCountMismatch { want, got } => {
                write!(f, "header promises {want} entities, found {got}")
            }
        }
    }
}

impl std::error::Error for DwgError {}
