//! Typed, build-validated UI messages. User arguments are inserted literally.
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Locale {
    #[default]
    En,
    Uk,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocaleError {
    MalformedTag,
}
impl fmt::Display for LocaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("malformed locale tag")
    }
}
impl std::error::Error for LocaleError {}
impl Locale {
    /// Accept a two-letter language and optional two-letter region.
    /// Unsupported, well-formed languages use English.
    pub fn parse(tag: &str) -> Result<Self, LocaleError> {
        let bytes = tag.as_bytes();
        if !((bytes.len() == 2 && bytes.iter().all(u8::is_ascii_alphabetic))
            || (bytes.len() == 5
                && bytes[2] == b'-'
                && bytes[..2]
                    .iter()
                    .chain(&bytes[3..])
                    .all(u8::is_ascii_alphabetic)))
        {
            return Err(LocaleError::MalformedTag);
        }
        Ok(if tag[..2].eq_ignore_ascii_case("uk") {
            Self::Uk
        } else {
            Self::En
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatError {
    ArgumentsRequired,
}
impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("message requires typed arguments")
    }
}
impl std::error::Error for FormatError {}

/// The one committed catalog, exposed for inspection only.
pub const CATALOG_JSON: &str = include_str!("../resources/messages.json");
include!(concat!(env!("OUT_DIR"), "/message_catalog.rs"));
