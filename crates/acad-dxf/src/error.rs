use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DxfError {
    InvalidString {
        field: &'static str,
    },
    UnsupportedFilletRadius,
    UnsupportedGroup {
        reason: &'static str,
    },
    InvalidLayerVisibility {
        layer: u8,
        color: Option<u8>,
    },
    InvalidLayerColor {
        layer: u8,
        value: i16,
    },
    InvalidLayerTable {
        layer: u8,
        color: u8,
    },
    UnsupportedNestedRepeat,
    Corrupt {
        offset: usize,
    },
    UnknownKeyword {
        keyword: String,
        line: usize,
    },
    Truncated {
        keyword: String,
        line: usize,
    },
    BadHeader {
        line: usize,
    },
    BadNumber {
        row: String,
        line: usize,
    },
    WrongFieldCount {
        keyword: String,
        want: usize,
        got: usize,
        line: usize,
    },
    UnterminatedBlock {
        name: String,
        line: usize,
    },
    UndefinedBlock {
        name: String,
    },
}

impl fmt::Display for DxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidString { field } => write!(f, "{field} cannot contain control characters in historical DXF"),
            Self::UnsupportedFilletRadius => f.write_str("historical DXF has no supported nonzero FILLET radius mapping"),
            Self::UnsupportedGroup { reason } => f.write_str(reason),
            Self::UnsupportedNestedRepeat => {
                write!(f, "nested REPEAT is unsupported by historical DXF readers")
            }
            Self::InvalidLayerVisibility { layer, color } => write!(f,
                "OFF layer {layer} (color {color:?}) requires a defined layer 1..127 with color 1..127"),
            Self::InvalidLayerColor { layer, value } => write!(f,
                "invalid LAYERC slot {layer}: signed color {value}"),
            Self::InvalidLayerTable { layer, color } => write!(f,
                "invalid layer table slot {layer} color {color}: indices must be 0..127 and colors 0..254"),
            Self::Corrupt { offset } => {
                write!(f, "non-text byte at offset {offset}: file is corrupt")
            }
            Self::UnknownKeyword { keyword, line } => {
                write!(f, "line {line}: unsupported record `{keyword}`")
            }
            Self::Truncated { keyword, line } => {
                write!(f, "line {line}: record `{keyword}` runs past end of file")
            }
            Self::BadHeader { line } => write!(f, "line {line}: expected `KEYWORD,<count>`"),
            Self::BadNumber { row, line } => {
                write!(f, "line {line}: cannot parse numbers from `{row}`")
            }
            Self::WrongFieldCount {
                keyword,
                want,
                got,
                line,
            } => write!(
                f,
                "line {line}: `{keyword}` needs {want} fields, found {got}"
            ),
            Self::UnterminatedBlock { name, line } => {
                write!(f, "line {line}: block `{name}` is never closed by ENDBLK")
            }
            Self::UndefinedBlock { name } => {
                write!(f, "INSERT references undefined block `{name}`")
            }
        }
    }
}

impl std::error::Error for DxfError {}
