use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DwgError {
    /// The first bytes are neither `AC1.2` nor `AC1.40`.
    UnknownVersion {
        found: [u8; 8],
    },
    /// The file is shorter than the fixed header.
    ShortHeader {
        len: usize,
        need: usize,
    },
    /// A header value cannot be represented by the drawing format's range.
    InvalidHeaderValue {
        field: &'static str,
        value: u16,
    },
    /// An entity's layer word cannot be represented by the model's layer ID.
    InvalidEntityLayer {
        index: u32,
        value: u16,
    },
    /// An entity record runs past the end of the file.
    TruncatedEntity {
        index: u32,
        at: usize,
        len: usize,
    },
    /// An entity type code this codec does not know.
    UnknownEntityType {
        code: u16,
        at: usize,
    },
    /// The header promises more entities than the bytes hold.
    EntityCountMismatch {
        want: u32,
        got: u32,
    },
    /// A `BLOCK` record with no matching `ENDBLK` before the entity region
    /// ends. Names the **innermost** still-open block (a `BLOCK` nested
    /// inside another is a sibling in the flat block table, not a child —
    /// see `read_items`'s doc comment — so several can be open at once) and
    /// the offset that block's own `BLOCK` record started at.
    UnterminatedBlock {
        name: String,
        at: usize,
    },
    /// An `ENDBLK` record with no `BLOCK` currently open. Names the stray
    /// `ENDBLK` record's own offset.
    StrayEndblk {
        at: usize,
    },
    UnterminatedRepeat {
        at: usize,
    },
    StrayEndrep {
        at: usize,
    },
    NestedRepeat {
        at: usize,
    },
    /// The record walk stopped at a byte offset that is not the header's
    /// own `entity_end` — either a record decoded past it, or `entity_end`
    /// was already at or before the fixed start of the entity region (a
    /// zeroed or corrupt header field) so the walk had nowhere valid to
    /// land. `pos` is where the walk actually stopped.
    WalkOverran {
        pos: usize,
        entity_end: usize,
    },
    /// A value in the model cannot be represented by this DWG writer.
    WriteValue {
        field: &'static str,
        value: String,
    },
}

impl fmt::Display for DwgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownVersion { found } => write!(
                f,
                "not a DWG: magic is {:?}, expected \"AC1.2\" or \"AC1.40\"",
                String::from_utf8_lossy(found)
            ),
            Self::ShortHeader { len, need } => {
                write!(f, "file is {len} bytes: the header alone needs {need}")
            }
            Self::InvalidHeaderValue { field, value } => write!(f, "invalid {field}: {value}"),
            Self::InvalidEntityLayer { index, value } => {
                write!(f, "entity {index} has invalid layer {value}")
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
            Self::UnterminatedBlock { name, at } => write!(
                f,
                "BLOCK \"{name}\" at offset {at:#x} has no matching ENDBLK"
            ),
            Self::StrayEndblk { at } => {
                write!(f, "ENDBLK at offset {at:#x} has no matching BLOCK")
            }
            Self::UnterminatedRepeat { at } => write!(f, "REPEAT at offset {at:#x} has no ENDREP"),
            Self::StrayEndrep { at } => write!(f, "ENDREP at offset {at:#x} has no REPEAT"),
            Self::NestedRepeat { at } => {
                write!(f, "nested REPEAT at offset {at:#x} is unsupported")
            }
            Self::WalkOverran { pos, entity_end } => write!(
                f,
                "entity walk stopped at offset {pos:#x}, not the header's own entity_end \
                 {entity_end:#x}"
            ),
            Self::WriteValue { field, value } => write!(f, "cannot write {field}: {value}"),
        }
    }
}

impl std::error::Error for DwgError {}
