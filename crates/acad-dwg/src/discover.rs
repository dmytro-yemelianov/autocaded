//! Locating known values inside an undocumented binary.
//!
//! The DXF sibling of a DWG says exactly which coordinates the drawing holds.
//! Searching the DWG for those values shows where each field lives, which is
//! how every record layout in this crate was recovered.

/// One place a searched-for value appears.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldHit {
    pub offset: usize,
    pub value: f64,
}

/// Every offset at which `needle` appears as a little-endian `f64`.
///
/// Steps one byte at a time: the records are not 8-byte aligned. `0.0` is never
/// reported — a DWG is mostly zero padding, so it would bury every real hit.
pub fn find_f64(bytes: &[u8], needle: f64, tolerance: f64) -> Vec<FieldHit> {
    if needle == 0.0 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for offset in 0..bytes.len().saturating_sub(7) {
        let value = f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if (value - needle).abs() <= tolerance {
            hits.push(FieldHit { offset, value });
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_with(values: &[(usize, f64)], len: usize) -> Vec<u8> {
        let mut b = vec![0u8; len];
        for (at, v) in values {
            b[*at..*at + 8].copy_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn finds_a_double_at_an_unaligned_offset() {
        // The format is not 8-byte aligned — a clean 1.0 sits at 0x19CD in
        // SUBDIV.DWG — so the search must step one byte at a time.
        let b = bytes_with(&[(0x0d, 6.822910)], 0x40);
        let hits = find_f64(&b, 6.822910, 1e-6);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].offset, 0x0d);
    }

    #[test]
    fn finds_every_occurrence_not_just_the_first() {
        let b = bytes_with(&[(0x10, 2.5), (0x30, 2.5)], 0x40);
        let offsets: Vec<usize> = find_f64(&b, 2.5, 1e-6).iter().map(|h| h.offset).collect();
        assert_eq!(offsets, vec![0x10, 0x30]);
    }

    #[test]
    fn tolerance_accounts_for_the_dxf_printing_six_decimals() {
        // The DXF prints 1.012459; the DWG holds the full double. A search that
        // demanded equality would miss it, which is how the first probe of this
        // file lost the LINE's x1.
        let b = bytes_with(&[(0x08, 1.0124585)], 0x20);
        assert_eq!(find_f64(&b, 1.012459, 1e-6).len(), 1);
        assert_eq!(find_f64(&b, 1.012459, 1e-12).len(), 0);
    }

    #[test]
    fn zero_is_not_reported_because_padding_is_full_of_it() {
        // A file is mostly zero bytes, so every 8-byte window of padding reads
        // as 0.0. Reporting those would bury every real hit.
        let b = bytes_with(&[], 0x40);
        assert!(find_f64(&b, 0.0, 1e-6).is_empty());
    }

    #[test]
    fn a_needle_near_the_end_does_not_read_past_it() {
        let b = bytes_with(&[(0x18, 7.0)], 0x20);
        assert_eq!(find_f64(&b, 7.0, 1e-6).len(), 1);
        assert!(find_f64(&b, 9.0, 1e-6).is_empty());
    }
}
