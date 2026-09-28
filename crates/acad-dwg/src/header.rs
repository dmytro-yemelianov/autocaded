use crate::DwgError;
use acad_model::{
    header::{DwgView, Header, Mode},
    Extents, Point,
};
use std::collections::BTreeMap;
use std::fmt;

/// Header field offsets, established against `SUBDIV.DXF` (spec §4.2).
const OFF_ENTITY_END: usize = 0x24;
const OFF_ENTITY_COUNT: usize = 0x28;
const OFF_EXTENTS: usize = 0x2a;
const OFF_LIMITS: usize = 0x5a;
const OFF_VIEW: usize = 0x7a;
const OFF_VIEW_HEIGHT: usize = 0x92;
const OFF_SNAP_FLAG: usize = 0x9a;
const OFF_SNAP: usize = 0x9c;
const OFF_GRID_FLAG: usize = 0xa4;
const OFF_GRID: usize = 0xa6;
const OFF_ORTHO: usize = 0xae;
const OFF_FILL: usize = 0xb0;
const OFF_TXTSIZE: usize = 0xb4;
const OFF_TRACEWID: usize = 0xbc;
/// The last field this codec reads, plus its width.
const HEADER_MIN: usize = OFF_TRACEWID + 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    Ac12,
    Ac140,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ac12 => "AC1.2",
            Self::Ac140 => "AC1.40",
        })
    }
}

impl Version {
    pub fn detect(bytes: &[u8]) -> Result<Version, DwgError> {
        if bytes.starts_with(b"AC1.40") {
            return Ok(Version::Ac140);
        }
        if bytes.starts_with(b"AC1.2") {
            return Ok(Version::Ac12);
        }
        let mut found = [0u8; 8];
        let n = bytes.len().min(8);
        found[..n].copy_from_slice(&bytes[..n]);
        // Only the magic itself is diagnostic; whatever follows its own null
        // terminator is unrelated file content, not part of the magic, so it
        // is zeroed rather than echoed back.
        if let Some(nul) = found[..n].iter().position(|&b| b == 0) {
            found[nul + 1..].fill(0);
        }
        Err(DwgError::UnknownVersion { found })
    }
}

/// What the header says about the entity region, which the entity reader needs
/// and `acad_model::Header` has no place for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderMeta {
    pub entity_count: u32,
    pub entity_end: u32,
}

fn f64_at(b: &[u8], at: usize) -> f64 {
    f64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

pub fn parse_header(bytes: &[u8]) -> Result<(Header, HeaderMeta), DwgError> {
    let version = Version::detect(bytes)?;
    if version != Version::Ac12 {
        return Err(DwgError::UnsupportedVersion {
            found: version,
            supported: Version::Ac12,
        });
    }
    if bytes.len() < HEADER_MIN {
        return Err(DwgError::ShortHeader {
            len: bytes.len(),
            need: HEADER_MIN,
        });
    }

    // EXTENTS is two 3D points — min (x, y, z) then max (x, y, z), so the
    // maxima are 24 and 32 bytes in, not 16 and 24. LIMITS is two 2D pairs.
    let extents = Extents {
        xmin: f64_at(bytes, OFF_EXTENTS),
        ymin: f64_at(bytes, OFF_EXTENTS + 8),
        xmax: f64_at(bytes, OFF_EXTENTS + 24),
        ymax: f64_at(bytes, OFF_EXTENTS + 32),
    };
    let limits = Extents {
        xmin: f64_at(bytes, OFF_LIMITS),
        ymin: f64_at(bytes, OFF_LIMITS + 8),
        xmax: f64_at(bytes, OFF_LIMITS + 16),
        ymax: f64_at(bytes, OFF_LIMITS + 24),
    };

    let header = Header {
        extents,
        limits,
        // BASE is 0,0 in SUBDIV, so its offset is not yet pinned — a field
        // whose only sample is zero cannot be located by searching for it.
        // Task 7 revisits this against a drawing with a non-zero BASE.
        base: Point { x: 0.0, y: 0.0 },
        view: DwgView {
            center: Point {
                x: f64_at(bytes, OFF_VIEW),
                y: f64_at(bytes, OFF_VIEW + 8),
            },
            height: f64_at(bytes, OFF_VIEW_HEIGHT),
        },
        snap: Mode {
            on: u16_at(bytes, OFF_SNAP_FLAG) != 0,
            spacing: f64_at(bytes, OFF_SNAP),
        },
        grid: Mode {
            on: u16_at(bytes, OFF_GRID_FLAG) != 0,
            spacing: f64_at(bytes, OFF_GRID),
        },
        ortho: u16_at(bytes, OFF_ORTHO) != 0,
        fill: u16_at(bytes, OFF_FILL) != 0,
        text_size: f64_at(bytes, OFF_TXTSIZE),
        trace_width: f64_at(bytes, OFF_TRACEWID),
        // The layer table follows the scalars and is reversed in Task 7, where
        // SUBDIV.DXF's LAYERC record is the oracle. Until then a drawing reads
        // as layer 0 with no table, which renders correctly because milestone
        // ① does not colour by layer yet.
        current_layer: 0,
        layers: BTreeMap::new(),
    };
    Ok((header, meta_of(bytes)))
}

fn meta_of(bytes: &[u8]) -> HeaderMeta {
    HeaderMeta {
        entity_count: u32_at(bytes, OFF_ENTITY_COUNT),
        entity_end: u32_at(bytes, OFF_ENTITY_END),
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic AC1.2 header: magic, the two u32s, then doubles at the
    /// offsets the spec records.
    fn header_bytes() -> Vec<u8> {
        let mut h = vec![0u8; 0x100];
        h[..6].copy_from_slice(b"AC1.2\0");
        h[0x24..0x28].copy_from_slice(&0x19e5u32.to_le_bytes());
        h[0x28..0x2c].copy_from_slice(&171u32.to_le_bytes());
        let put = |h: &mut Vec<u8>, at: usize, v: f64| {
            h[at..at + 8].copy_from_slice(&v.to_le_bytes());
        };
        // EXTENTS min (x, y, z) then max (x, y, z)
        put(&mut h, 0x2a, -1.75);
        put(&mut h, 0x32, -1.75);
        put(&mut h, 0x3a, 0.0);
        put(&mut h, 0x42, 19.0);
        put(&mut h, 0x4a, 14.0);
        put(&mut h, 0x52, 0.0);
        // LIMITS min (x, y) then max (x, y)
        put(&mut h, 0x5a, -2.0);
        put(&mut h, 0x62, -2.0);
        put(&mut h, 0x6a, 19.0);
        put(&mut h, 0x72, 14.0);
        h
    }

    #[test]
    fn detects_the_two_versions_by_magic() {
        assert_eq!(Version::detect(b"AC1.2\0rest").unwrap(), Version::Ac12);
        assert_eq!(Version::detect(b"AC1.40\0rest").unwrap(), Version::Ac140);
    }

    #[test]
    fn an_unknown_magic_is_a_named_error() {
        assert_eq!(
            Version::detect(b"AC1.50\0junk").unwrap_err(),
            DwgError::UnknownVersion {
                found: *b"AC1.50\0\0"
            }
        );
    }

    #[test]
    fn ac140_is_rejected_with_a_message_naming_both_versions() {
        // Review Focus 1: five corpus drawings are AC1.40 and this codec reads
        // AC1.2. Opening one must say so, not misparse.
        let mut b = header_bytes();
        b[..7].copy_from_slice(b"AC1.40\0");
        assert_eq!(
            parse_header(&b).unwrap_err(),
            DwgError::UnsupportedVersion {
                found: Version::Ac140,
                supported: Version::Ac12
            }
        );
    }

    #[test]
    fn reads_the_entity_count_and_end_offset() {
        let (_, meta) = parse_header(&header_bytes()).unwrap();
        assert_eq!(meta.entity_count, 171);
        assert_eq!(meta.entity_end, 0x19e5);
    }

    #[test]
    fn reads_extents_and_limits_at_the_recorded_offsets() {
        let (header, _) = parse_header(&header_bytes()).unwrap();
        assert_eq!(
            header.extents,
            Extents {
                xmin: -1.75,
                xmax: 19.0,
                ymin: -1.75,
                ymax: 14.0
            }
        );
        assert_eq!(
            header.limits,
            Extents {
                xmin: -2.0,
                xmax: 19.0,
                ymin: -2.0,
                ymax: 14.0
            }
        );
    }

    #[test]
    fn reads_the_remaining_scalar_header_fields() {
        // DWGVIEW 8.5, 6.163380, 16.326761; MODERES and MODEGRID 0.25;
        // TXTSIZE 0.2; TRACEWID 0.05 — all from SUBDIV.DXF's header.
        let mut h = header_bytes();
        let put = |h: &mut Vec<u8>, at: usize, v: f64| {
            h[at..at + 8].copy_from_slice(&v.to_le_bytes());
        };
        put(&mut h, 0x7a, 8.5);
        put(&mut h, 0x82, 6.163380);
        put(&mut h, 0x92, 16.326761);
        h[0x9a..0x9c].copy_from_slice(&0u16.to_le_bytes());
        put(&mut h, 0x9c, 0.25);
        h[0xa4..0xa6].copy_from_slice(&0u16.to_le_bytes());
        put(&mut h, 0xa6, 0.25);
        put(&mut h, 0xb4, 0.2);
        put(&mut h, 0xbc, 0.05);

        let (header, _) = parse_header(&h).unwrap();
        assert_eq!(
            header.view.center,
            Point {
                x: 8.5,
                y: 6.163380
            }
        );
        assert_eq!(header.view.height, 16.326761);
        assert_eq!(
            header.snap,
            Mode {
                on: false,
                spacing: 0.25
            }
        );
        assert_eq!(
            header.grid,
            Mode {
                on: false,
                spacing: 0.25
            }
        );
        assert_eq!(header.text_size, 0.2);
        assert_eq!(header.trace_width, 0.05);
    }

    #[test]
    fn a_file_shorter_than_the_header_is_an_error_not_a_panic() {
        // Review Focus 2.
        let short = header_bytes()[..0x40].to_vec();
        assert_eq!(
            parse_header(&short).unwrap_err(),
            DwgError::ShortHeader {
                len: 0x40,
                need: 0xc4
            }
        );
    }
}
