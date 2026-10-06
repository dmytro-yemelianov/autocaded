//! Strongly-typed command dispatch and editing errors for acad-cmd.

use std::fmt;

/// Semantic presentation attached to one command submission; transport stays English.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDiagnostic {
    pub kind: CmdErrorKind,
    pub message: crate::messages::Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmdErrorKind {
    Syntax,
    UnknownCommand,
    InvalidPoint,
    InvalidScalar,
    InvalidSelection,
    NoSuchBlock,
    NoSuchLayer,
    LimitsExceeded,
    Io,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CmdError {
    pub kind: CmdErrorKind,
    pub message: String,
}

impl CmdError {
    pub fn new(kind: CmdErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn syntax(message: impl Into<String>) -> Self {
        Self::new(CmdErrorKind::Syntax, message)
    }

    pub fn unknown_command(message: impl Into<String>) -> Self {
        Self::new(CmdErrorKind::UnknownCommand, message)
    }

    pub fn invalid_point(message: impl Into<String>) -> Self {
        Self::new(CmdErrorKind::InvalidPoint, message)
    }

    pub fn invalid_scalar(message: impl Into<String>) -> Self {
        Self::new(CmdErrorKind::InvalidScalar, message)
    }

    pub fn invalid_selection(message: impl Into<String>) -> Self {
        Self::new(CmdErrorKind::InvalidSelection, message)
    }

    pub fn no_such_block(message: impl Into<String>) -> Self {
        Self::new(CmdErrorKind::NoSuchBlock, message)
    }

    pub fn no_such_layer(message: impl Into<String>) -> Self {
        Self::new(CmdErrorKind::NoSuchLayer, message)
    }

    pub fn limits_exceeded(message: impl Into<String>) -> Self {
        Self::new(CmdErrorKind::LimitsExceeded, message)
    }

    pub fn classify(message: String) -> Self {
        let lower = message.to_ascii_lowercase();
        let kind = if lower.contains("unknown command") || lower.contains("bad command") {
            CmdErrorKind::UnknownCommand
        } else if lower.contains("point") || lower.contains("coordinate") {
            CmdErrorKind::InvalidPoint
        } else if lower.contains("select")
            || lower.contains("no entity")
            || lower.contains("entities found")
        {
            CmdErrorKind::InvalidSelection
        } else if lower.contains("block") {
            CmdErrorKind::NoSuchBlock
        } else if lower.contains("layer") {
            CmdErrorKind::NoSuchLayer
        } else if lower.contains("limits") {
            CmdErrorKind::LimitsExceeded
        } else if lower.contains("number")
            || lower.contains("decimal")
            || lower.contains("positive")
        {
            CmdErrorKind::InvalidScalar
        } else if lower.contains("syntax") || lower.contains("invalid") {
            CmdErrorKind::Syntax
        } else {
            CmdErrorKind::Other
        };
        Self { kind, message }
    }
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CmdError {}

impl From<String> for CmdError {
    fn from(s: String) -> Self {
        Self::classify(s)
    }
}

impl From<&str> for CmdError {
    fn from(s: &str) -> Self {
        Self::classify(s.to_string())
    }
}

impl From<CmdError> for String {
    fn from(err: CmdError) -> Self {
        err.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification() {
        let err1 = CmdError::from("Unknown command 'FOO'");
        assert_eq!(err1.kind, CmdErrorKind::UnknownCommand);

        let err2 = CmdError::from("Invalid 2D point format");
        assert_eq!(err2.kind, CmdErrorKind::InvalidPoint);

        let err3 = CmdError::from("No entities found in selection window");
        assert_eq!(err3.kind, CmdErrorKind::InvalidSelection);

        let err4 = CmdError::from("Block 'DOOR' not found");
        assert_eq!(err4.kind, CmdErrorKind::NoSuchBlock);

        let err5 = CmdError::from("Layer 'WALLS' does not exist");
        assert_eq!(err5.kind, CmdErrorKind::NoSuchLayer);
    }
}
