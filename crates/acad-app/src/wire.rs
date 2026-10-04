//! Bounded newline framing used by local transports.
use std::io::{BufRead, Read};
pub(crate) fn read_message(reader: &mut impl BufRead) -> Result<Option<String>, String> {
    let mut bytes = Vec::new();
    let n = reader
        .take(65_537)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(None);
    }
    if bytes.len() > 65_536 {
        return Err("request exceeds 64 KiB".into());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|e| e.to_string())
}

pub(crate) fn write_message(
    output: &mut impl std::io::Write,
    value: &serde_json::Value,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    output.write_all(&bytes).map_err(|e| e.to_string())
}
