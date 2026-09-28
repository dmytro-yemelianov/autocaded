//! DWG text is Latin-1, not UTF-8 — the dispatch's own Global Constraints
//! note that a 1983 binary format has no escaping to fall back on the way a
//! text format might, so a byte with the high bit set is either a real
//! Latin-1 character or nothing at all; it is never invalid the way it would
//! be as a UTF-8 lead byte.

/// Decodes `bytes` as Latin-1: every byte is its own code point, so this
/// never fails the way UTF-8 decoding can. Stops at the first `0x00`, which
/// is how some DWG string fields pad or terminate short of their slot's full
/// width; a length-prefixed field (`entity.rs`'s `BLOCK`/`INSERT`/`TEXT`
/// strings) is sliced to its declared length before reaching here, so it
/// never contains one, and this is a no-op for it.
pub fn decode_latin1(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_decodes_as_latin1_not_utf8() {
        // 0xE9 is é in Latin-1 and an invalid UTF-8 lead byte. Decoding as
        // UTF-8 would either fail or produce a replacement character.
        assert_eq!(decode_latin1(&[b'c', b'a', b'f', 0xe9]), "café");
    }

    #[test]
    fn a_nul_terminates_a_text_field() {
        assert_eq!(
            decode_latin1(&[b'h', b'i', 0, b'j', b'u', b'n', b'k']),
            "hi"
        );
    }
}
