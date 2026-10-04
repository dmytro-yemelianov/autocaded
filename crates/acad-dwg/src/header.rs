use crate::DwgError;
use acad_model::{
    header::{DwgView, Header, Mode, UnitFormat, Units},
    Extents, Point,
};
use std::collections::BTreeMap;
use std::fmt;

/// Header field offsets, established against `SUBDIV.DXF` (spec §4.2).
/// Non-zero BASE is independently located by generated AC1.40 drawings.
const OFF_BASE: usize = 0x0c;
const OFF_ENTITY_END: usize = 0x24;
/// A `u16`, not `u32` — see the comment where it is read in `meta_of`.
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
const OFF_FILL: usize = 0xb2;
const OFF_TXTSIZE: usize = 0xb4;
const OFF_TRACEWID: usize = 0xbc;
const OFF_CURRENT_LAYER: usize = 0xc4;
const OFF_LAYERS: usize = 0xc8;
const OFF_DIM_ARROW: usize = 0x1c8;
// Retained FILLET state DS:48fe is descriptor42, final AC1.40 field.
const OFF_FILLET_RADIUS: usize = 0x1fa;
const OFF_AXIS_FLAG: usize = 0x1e0;
const OFF_AXIS_SPACING: usize = 0x1e2;
const OFF_UNITS_FORMAT: usize = 0x1d8;
const OFF_UNITS_PRECISION: usize = 0x1da;
const LAYER_SLOTS: usize = 128;

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
    /// AC1.40 adds 42 header bytes before the otherwise shared entity layout.
    /// Established against generated drawings and original sample exports.
    pub fn entity_start(self) -> usize {
        match self {
            Self::Ac12 => 0x1d8,
            Self::Ac140 => 0x202,
        }
    }

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
    pub version: Version,
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
    let header_min = version.entity_start();
    if bytes.len() < header_min {
        return Err(DwgError::ShortHeader {
            len: bytes.len(),
            need: header_min,
        });
    }

    let layer = u16_at(bytes, OFF_CURRENT_LAYER);
    if layer as usize >= LAYER_SLOTS {
        return Err(DwgError::InvalidHeaderValue {
            field: "current layer",
            value: layer,
        });
    }
    let mut layers = BTreeMap::new();
    let mut off_layers = std::collections::BTreeSet::new();
    for slot in 0..LAYER_SLOTS {
        let raw = u16_at(bytes, OFF_LAYERS + slot * 2);
        let signed = raw as i16;
        let color = if signed < 0 {
            // Retained historical OFF is NEG, not a sign-bit flag. Keep native
            // zero/high-color extensions outside this verified signed range.
            let magnitude = signed.checked_neg().filter(|&n| (1..=127).contains(&n));
            if slot == 0 || magnitude.is_none() {
                return Err(DwgError::InvalidHeaderValue {
                    field: "OFF layer color",
                    value: raw,
                });
            }
            off_layers.insert(slot as u8);
            magnitude.unwrap() as u16
        } else {
            raw
        };
        if color > 255 {
            return Err(DwgError::InvalidHeaderValue {
                field: "layer color",
                value: raw,
            });
        }
        if color != 255 {
            layers.insert(slot as u8, color as u8);
        }
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
        base: Point {
            x: f64_at(bytes, OFF_BASE),
            y: f64_at(bytes, OFF_BASE + 8),
        },
        view: DwgView {
            center: Point {
                x: f64_at(bytes, OFF_VIEW),
                y: f64_at(bytes, OFF_VIEW + 8),
            },
            height: f64_at(bytes, OFF_VIEW_HEIGHT),
        },
        axis: if version == Version::Ac140 {
            Mode {
                on: u16_at(bytes, OFF_AXIS_FLAG) != 0,
                spacing: f64_at(bytes, OFF_AXIS_SPACING),
            }
        } else {
            Mode {
                on: false,
                spacing: 0.0,
            }
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
        fillet_radius: if version == Version::Ac140 {
            f64_at(bytes, OFF_FILLET_RADIUS)
        } else {
            0.0
        },
        // Positive-zero bits accompany omitted native exports in all 50
        // menu-control pairs. Preserve negative zero and every other value.
        dim_arrow: if version == Version::Ac140 {
            let value = f64_at(bytes, OFF_DIM_ARROW);
            (value.to_bits() != 0).then_some(value)
        } else {
            None
        },
        units: if version == Version::Ac140 {
            Units {
                format: UnitFormat::from_disk(u16_at(bytes, OFF_UNITS_FORMAT)),
                precision: u16_at(bytes, OFF_UNITS_PRECISION),
            }
        } else {
            Units {
                format: UnitFormat::Decimal,
                precision: 4,
            }
        },
        current_layer: layer as u8,
        layers,
        off_layers,
        dwg_header_passthrough: Some(bytes[..header_min].to_vec()),
    };
    if !header.fillet_radius.is_finite() || header.fillet_radius < 0.0 {
        return Err(DwgError::InvalidHeaderScalar {
            field: "FILLET radius",
            value: header.fillet_radius.to_string(),
        });
    }
    Ok((header, meta_of(bytes, version)))
}

fn meta_of(bytes: &[u8], version: Version) -> HeaderMeta {
    HeaderMeta {
        version,
        // A u16, not a u32: SUBDIV's xmin (-1.75) is exactly representable,
        // so its low mantissa bytes at 0x2a-0x2b happen to be zero, which
        // made a wider read look correct until other corpus files (whose
        // xmin isn't a clean fraction) exposed it — see spec §4.2.
        entity_count: u16_at(bytes, OFF_ENTITY_COUNT) as u32,
        entity_end: u32_at(bytes, OFF_ENTITY_END),
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic AC1.2 header: magic, the entity-end `u32` and the
    /// entity-count `u16`, then doubles at the offsets the spec records.
    fn header_bytes() -> Vec<u8> {
        let mut h = vec![0u8; 0x202];
        h[..6].copy_from_slice(b"AC1.2\0");
        h[0x24..0x28].copy_from_slice(&0x19e5u32.to_le_bytes());
        h[0x28..0x2a].copy_from_slice(&171u16.to_le_bytes());
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
    fn ac140_uses_its_extended_header() {
        let mut b = header_bytes();
        b[..7].copy_from_slice(b"AC1.40\0");
        assert_eq!(parse_header(&b).unwrap().1.version.entity_start(), 0x202);
        b.truncate(0x1d8);
        assert_eq!(
            parse_header(&b).unwrap_err(),
            DwgError::ShortHeader {
                len: 0x1d8,
                need: 0x202
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
    fn reads_ac140_units_format_and_precision() {
        let mut h = header_bytes();
        h[..7].copy_from_slice(b"AC1.40\0");
        h[OFF_UNITS_FORMAT..OFF_UNITS_FORMAT + 2].copy_from_slice(&4u16.to_le_bytes());
        h[OFF_UNITS_PRECISION..OFF_UNITS_PRECISION + 2].copy_from_slice(&16u16.to_le_bytes());
        let (header, _) = parse_header(&h).unwrap();
        assert_eq!(
            header.units,
            Units {
                format: UnitFormat::Architectural,
                precision: 16
            }
        );
    }

    #[test]
    fn a_file_shorter_than_the_header_is_an_error_not_a_panic() {
        // Review Focus 2.
        let short = header_bytes()[..0x40].to_vec();
        assert_eq!(
            parse_header(&short).unwrap_err(),
            DwgError::ShortHeader {
                len: 0x40,
                need: 0x1d8
            }
        );
    }

    #[test]
    fn layer_values_outside_their_ranges_are_rejected() {
        let mut h = header_bytes();
        h[OFF_CURRENT_LAYER..OFF_CURRENT_LAYER + 2].copy_from_slice(&128u16.to_le_bytes());
        assert_eq!(
            parse_header(&h).unwrap_err(),
            DwgError::InvalidHeaderValue {
                field: "current layer",
                value: 128
            }
        );
        h[OFF_CURRENT_LAYER..OFF_CURRENT_LAYER + 2].copy_from_slice(&0u16.to_le_bytes());
        h[OFF_LAYERS..OFF_LAYERS + 2].copy_from_slice(&256u16.to_le_bytes());
        assert_eq!(
            parse_header(&h).unwrap_err(),
            DwgError::InvalidHeaderValue {
                field: "layer color",
                value: 256
            }
        );
    }
}
