use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DxfError {
    Corrupt { offset: usize },
    UnknownKeyword { keyword: String, line: usize },
    Truncated { keyword: String, line: usize },
    BadHeader { line: usize },
    BadNumber { row: String, line: usize },
    UndefinedBlock { name: String },
}

impl fmt::Display for DxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt { offset } =>
                write!(f, "non-text byte at offset {offset}: file is corrupt"),
            Self::UnknownKeyword { keyword, line } =>
                write!(f, "line {line}: unsupported record `{keyword}`"),
            Self::Truncated { keyword, line } =>
                write!(f, "line {line}: record `{keyword}` runs past end of file"),
            Self::BadHeader { line } =>
                write!(f, "line {line}: expected `KEYWORD,<count>`"),
            Self::BadNumber { row, line } =>
                write!(f, "line {line}: cannot parse numbers from `{row}`"),
            Self::UndefinedBlock { name } =>
                write!(f, "INSERT references undefined block `{name}`"),
        }
    }
}

impl std::error::Error for DxfError {}
