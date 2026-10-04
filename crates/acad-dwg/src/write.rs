//! AC1.2/AC1.40 encoder. The fixed header and entity layouts mirror the
//! offsets decoded by `header` and `entity`; opaque original header bytes are
//! carried through where available, entity-end is logical, and output is
//! padded to the original 128-byte page boundary.
use crate::{header::Version, DwgError};
use acad_model::{Drawing, Entity, Item, Point};

fn put_u16(out: &mut [u8], at: usize, value: u16) {
    out[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put_u32(out: &mut [u8], at: usize, value: u32) {
    out[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_point(out: &mut Vec<u8>, p: Point) {
    put_f64(out, p.x);
    put_f64(out, p.y);
}

fn latin1(s: &str, field: &'static str) -> Result<Vec<u8>, DwgError> {
    s.chars()
        .map(|c| {
            u8::try_from(c as u32).map_err(|_| DwgError::WriteValue {
                field,
                value: format!("{c:?} is outside Latin-1"),
            })
        })
        .collect()
}

fn put_string(out: &mut Vec<u8>, s: &str, field: &'static str) -> Result<(), DwgError> {
    let bytes = latin1(s, field)?;
    let len = u16::try_from(bytes.len()).map_err(|_| DwgError::WriteValue {
        field,
        value: format!("{} bytes exceeds 65535", bytes.len()),
    })?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&bytes);
    Ok(())
}

fn record_header(out: &mut Vec<u8>, kind: u16, layer: u8, erased: bool) {
    let signed = if erased { -(kind as i16) } else { kind as i16 };
    out.extend_from_slice(&signed.to_le_bytes());
    out.extend_from_slice(&(layer as u16).to_le_bytes());
}

fn encode_entity(
    out: &mut Vec<u8>,
    entity: &Entity,
    count: &mut u32,
    version: Version,
    erased: bool,
) -> Result<(), DwgError> {
    let record =
        acad_model::group_codec::stored_record(entity).map_err(|error| DwgError::WriteValue {
            field: "entity",
            value: error.message().into(),
        })?;
    let layer = record.layer;
    let entity = record.entity;
    let erased = erased || record.erased;
    let angle = |deg: f64| deg.to_radians();
    match entity {
        Entity::Repeat(repeat) => {
            encode_repeat(out, repeat, count, version, erased)?;
            return Ok(());
        }
        Entity::OnLayer { .. } | Entity::Erased(_) => {
            unreachable!("checked wrappers were removed above")
        }
        Entity::Line { start, end } => {
            record_header(out, 1, layer, erased);
            put_point(out, *start);
            put_point(out, *end);
        }
        Entity::Point { origin } => {
            record_header(out, 2, layer, erased);
            put_point(out, *origin);
        }
        Entity::Circle { center, radius } => {
            record_header(out, 3, layer, erased);
            put_point(out, *center);
            put_f64(out, *radius);
        }
        Entity::Shape {
            origin,
            height,
            rotation_deg,
            number,
        } => {
            record_header(out, 4, layer, erased);
            put_point(out, *origin);
            put_f64(out, *height);
            put_f64(out, angle(*rotation_deg));
            out.extend_from_slice(&number.to_le_bytes());
        }
        Entity::Text {
            origin,
            height,
            rotation_deg,
            value,
        } => {
            let stored_height = *height
                * if version == Version::Ac12 {
                    4.0 / 3.0
                } else {
                    1.0
                };
            if !stored_height.is_finite() {
                return Err(DwgError::WriteValue {
                    field: "TEXT height",
                    value: "revision scaling is nonfinite".into(),
                });
            }
            record_header(out, 7, layer, erased);
            put_point(out, *origin);
            put_f64(out, stored_height);
            put_f64(out, angle(*rotation_deg));
            put_string(out, value, "TEXT")?;
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            record_header(out, 8, layer, erased);
            put_point(out, *center);
            put_f64(out, *radius);
            put_f64(out, angle(*start_deg));
            put_f64(out, angle(*end_deg));
        }
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            record_header(
                out,
                if matches!(entity, Entity::Trace { .. }) {
                    9
                } else {
                    11
                },
                layer,
                erased,
            );
            for p in [p1, p2, p3, p4] {
                put_point(out, *p);
            }
        }
        Entity::Load { name } => {
            record_header(out, 10, layer, erased);
            put_string(out, name, "LOAD")?;
        }
        Entity::Insert {
            origin,
            x_scale,
            y_scale,
            rotation_deg,
            name,
        } => {
            record_header(out, 14, layer, erased);
            put_string(out, name, "INSERT")?;
            put_point(out, *origin);
            put_f64(out, *x_scale);
            put_f64(out, *y_scale);
            put_f64(out, angle(*rotation_deg));
        }
        Entity::Generic(g) => {
            return Err(DwgError::WriteValue {
                field: "entity",
                value: format!(
                    "entity type {:?} is not supported in 1983 AutoCAD DWG format",
                    g.type_name
                ),
            });
        }
        Entity::Extension(ext) => {
            return Err(DwgError::WriteValue {
                field: "entity",
                value: format!(
                    "custom entity {:?} is not supported in 1983 AutoCAD DWG format",
                    ext.type_name()
                ),
            });
        }
    }
    *count += 1;
    Ok(())
}


fn encode_block(
    out: &mut Vec<u8>,
    name: &str,
    base: Point,
    entities: &[Entity],
    count: &mut u32,
    version: Version,
) -> Result<(), DwgError> {
    record_header(out, 12, 1, false);
    put_string(out, name, "BLOCK name")?;
    put_point(out, base);
    *count += 1;
    for entity in entities {
        encode_entity(out, entity, count, version, false)?;
    }
    record_header(out, 13, 1, false);
    *count += 1;
    Ok(())
}

fn encode_repeat(
    out: &mut Vec<u8>,
    repeat: &acad_model::Repeat,
    count: &mut u32,
    version: Version,
    erased: bool,
) -> Result<(), DwgError> {
    record_header(out, 5, repeat.start_layer, erased);
    *count += 1;
    for entity in &repeat.entities {
        encode_entity(out, entity, count, version, erased)?;
    }
    record_header(out, 6, repeat.end_layer, erased);
    out.extend_from_slice(&repeat.columns.to_le_bytes());
    out.extend_from_slice(&repeat.rows.to_le_bytes());
    put_f64(out, repeat.column_spacing);
    put_f64(out, repeat.row_spacing);
    *count += 1;
    Ok(())
}

/// Return a fully padded AC1.40 byte stream.
pub fn encode(drawing: &Drawing) -> Result<Vec<u8>, DwgError> {
    encode_version(drawing, Version::Ac140)
}

/// Return a fully padded DWG byte stream in the requested revision.
pub fn encode_version(drawing: &Drawing, version: Version) -> Result<Vec<u8>, DwgError> {
    acad_model::group_codec::validate_group_records(drawing).map_err(|error| {
        DwgError::WriteValue {
            field: "REPEAT",
            value: error.message().into(),
        }
    })?;
    let start = version.entity_start();
    let h = &drawing.header;
    if !h.fillet_radius.is_finite()
        || h.fillet_radius < 0.0
        || (version == Version::Ac12 && h.fillet_radius != 0.0)
    {
        return Err(DwgError::WriteValue {
            field: "FILLET radius",
            value: if version == Version::Ac12 {
                "AC1.2 has no supported nonzero radius mapping".into()
            } else {
                h.fillet_radius.to_string()
            },
        });
    }
    if let Some(value) = h.dim_arrow {
        if version == Version::Ac12 || !value.is_finite() {
            return Err(DwgError::WriteValue {
                field: "DIMARROW",
                value: if version == Version::Ac12 {
                    "AC1.2 has no supported mapping".into()
                } else {
                    value.to_string()
                },
            });
        }
    }
    if h.current_layer >= 128 {
        return Err(DwgError::WriteValue {
            field: "current layer",
            value: h.current_layer.to_string(),
        });
    }
    if h.layers.iter().any(|(&i, &c)| i >= 128 || c == 255) {
        return Err(DwgError::WriteValue {
            field: "layer table",
            value: "indices must be 0..127 and colors 0..254".into(),
        });
    }
    for &layer in &h.off_layers {
        if layer == 0
            || layer >= 128
            || !h
                .layers
                .get(&layer)
                .is_some_and(|color| (1..=127).contains(color))
        {
            return Err(DwgError::WriteValue {
                field: "layer visibility",
                value: format!(
                    "OFF layer {layer} requires a defined layer 1..127 with color 1..127"
                ),
            });
        }
    }
    let mut out = h
        .dwg_header_passthrough
        .as_ref()
        .map(|bytes| bytes.iter().copied().take(start).collect::<Vec<_>>())
        .unwrap_or_default();
    out.resize(start, 0);
    match version {
        Version::Ac12 => out[..6].copy_from_slice(b"AC1.2\0"),
        Version::Ac140 => out[..7].copy_from_slice(b"AC1.40\0"),
    }
    let mut entities = Vec::new();
    let mut count = 0u32;
    for item in &drawing.items {
        match item {
            Item::Entity(e) => encode_entity(&mut entities, e, &mut count, version, false)?,
            Item::Erased(e) => encode_entity(&mut entities, e, &mut count, version, true)?,
            Item::Repeat(r) => encode_repeat(&mut entities, r, &mut count, version, false)?,
            Item::Block(b) => encode_block(
                &mut entities,
                &b.name,
                b.base,
                &b.entities,
                &mut count,
                version,
            )?,
        }
    }
    let count16 = u16::try_from(count).map_err(|_| DwgError::WriteValue {
        field: "entity count",
        value: count.to_string(),
    })?;
    let end = start
        .checked_add(entities.len())
        .ok_or_else(|| DwgError::WriteValue {
            field: "entity end",
            value: "overflow".into(),
        })?;
    let end32 = u32::try_from(end).map_err(|_| DwgError::WriteValue {
        field: "entity end",
        value: end.to_string(),
    })?;

    put_f64_at(&mut out, 0x0c, h.base.x);
    put_f64_at(&mut out, 0x14, h.base.y);
    put_u32(&mut out, 0x24, end32);
    put_u16(&mut out, 0x28, count16);
    for (at, v) in [
        (0x2a, h.extents.xmin),
        (0x32, h.extents.ymin),
        (0x42, h.extents.xmax),
        (0x4a, h.extents.ymax),
        (0x5a, h.limits.xmin),
        (0x62, h.limits.ymin),
        (0x6a, h.limits.xmax),
        (0x72, h.limits.ymax),
        (0x7a, h.view.center.x),
        (0x82, h.view.center.y),
        (0x92, h.view.height),
        (0x9c, h.snap.spacing),
        (0xa6, h.grid.spacing),
        (0xb4, h.text_size),
        (0xbc, h.trace_width),
    ] {
        put_f64_at(&mut out, at, v);
    }
    put_u16(&mut out, 0x9a, h.snap.on as u16);
    put_u16(&mut out, 0xa4, h.grid.on as u16);
    put_u16(&mut out, 0xae, h.ortho as u16);
    put_u16(&mut out, 0xb2, h.fill as u16);
    put_u16(&mut out, 0xc4, h.current_layer as u16);
    if version == Version::Ac140 {
        put_f64_at(&mut out, 0x1fa, h.fillet_radius);
        if let Some(value) = h.dim_arrow {
            put_f64_at(&mut out, 0x1c8, value);
        }
        put_u16(&mut out, 0x1d8, h.units.format.disk_value());
        put_u16(&mut out, 0x1da, h.units.precision);
        put_u16(&mut out, 0x1e0, h.axis.on as u16);
        put_f64_at(&mut out, 0x1e2, h.axis.spacing);
    }
    for slot in 0..128 {
        put_u16(&mut out, 0xc8 + slot * 2, 255);
    }
    for (&slot, &color) in &h.layers {
        let signed = if h.off_layers.contains(&slot) {
            -(color as i16)
        } else {
            color as i16
        };
        put_u16(&mut out, 0xc8 + slot as usize * 2, signed as u16);
    }
    out.extend_from_slice(&entities);
    let padded = end.div_ceil(128) * 128;
    out.resize(padded, 0);
    Ok(out)
}

fn put_f64_at(out: &mut [u8], at: usize, value: f64) {
    out[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_reads_back_layered_geometry_and_blocks() {
        let zero = acad_model::Extents {
            xmin: -2.0,
            xmax: 8.0,
            ymin: -3.0,
            ymax: 9.0,
        };
        let d = Drawing {
            header: acad_model::Header {
                extents: zero,
                limits: zero,
                base: Point { x: 1.0, y: 2.0 },
                view: acad_model::DwgView {
                    center: Point { x: 3.0, y: 4.0 },
                    height: 10.0,
                },
                axis: acad_model::Mode {
                    on: false,
                    spacing: 0.0,
                },
                snap: acad_model::Mode {
                    on: true,
                    spacing: 0.5,
                },
                grid: acad_model::Mode {
                    on: false,
                    spacing: 1.0,
                },
                ortho: true,
                fill: true,
                text_size: 0.2,
                trace_width: 0.1,
                fillet_radius: 0.0,
                dim_arrow: None,
                units: acad_model::Units {
                    format: acad_model::UnitFormat::Decimal,
                    precision: 4,
                },
                current_layer: 2,
                layers: [(1, 7), (2, 64)].into_iter().collect(),
                off_layers: Default::default(),
                dwg_header_passthrough: None,
            },
            items: vec![
                Item::Entity(Entity::OnLayer {
                    layer: 2,
                    entity: Box::new(Entity::Line {
                        start: Point { x: 1.0, y: 2.0 },
                        end: Point { x: 3.0, y: 4.0 },
                    }),
                }),
                Item::Block(acad_model::Block {
                    name: "B".into(),
                    base: Point { x: 0.5, y: -0.5 },
                    entities: vec![Entity::OnLayer {
                        layer: 1,
                        entity: Box::new(Entity::Circle {
                            center: Point { x: 5.0, y: 6.0 },
                            radius: 2.0,
                        }),
                    }],
                }),
            ],
        };
        let bytes = encode(&d).unwrap();
        assert_eq!(bytes.len() % 128, 0);
        let decoded = crate::parse(&bytes).unwrap();
        assert_eq!(decoded, d);
        assert_eq!(u16::from_le_bytes(bytes[0x28..0x2a].try_into().unwrap()), 4);
        assert_eq!(
            u16::from_le_bytes(bytes[0x1d8..0x1da].try_into().unwrap()),
            2
        );
        assert_eq!(
            u16::from_le_bytes(bytes[0x1da..0x1dc].try_into().unwrap()),
            4
        );
        let end = u32::from_le_bytes(bytes[0x24..0x28].try_into().unwrap()) as usize;
        assert!(end < bytes.len());
        assert!(bytes[end..].iter().all(|&b| b == 0));
    }

    #[test]
    fn rejects_strings_that_are_not_latin1() {
        let err = latin1("snowman ☃", "TEXT").unwrap_err();
        assert!(matches!(err, DwgError::WriteValue { field: "TEXT", .. }));
    }

    #[test]
    fn ac12_writer_uses_legacy_start_and_text_cell_height() {
        let d = Drawing {
            header: acad_model::Header {
                extents: acad_model::Extents {
                    xmin: 0.0,
                    xmax: 10.0,
                    ymin: 0.0,
                    ymax: 10.0,
                },
                limits: acad_model::Extents {
                    xmin: 0.0,
                    xmax: 10.0,
                    ymin: 0.0,
                    ymax: 10.0,
                },
                base: Point { x: 0.0, y: 0.0 },
                view: acad_model::DwgView {
                    center: Point { x: 5.0, y: 5.0 },
                    height: 10.0,
                },
                axis: acad_model::Mode {
                    on: false,
                    spacing: 0.0,
                },
                snap: acad_model::Mode {
                    on: false,
                    spacing: 1.0,
                },
                grid: acad_model::Mode {
                    on: false,
                    spacing: 1.0,
                },
                ortho: false,
                fill: true,
                text_size: 1.0,
                trace_width: 0.25,
                fillet_radius: 0.0,
                dim_arrow: None,
                units: acad_model::Units {
                    format: acad_model::UnitFormat::Decimal,
                    precision: 4,
                },
                current_layer: 1,
                layers: Default::default(),
                off_layers: Default::default(),
                dwg_header_passthrough: None,
            },
            items: vec![Item::Entity(Entity::Text {
                origin: Point { x: 2.0, y: 3.0 },
                height: 0.75,
                rotation_deg: 90.0,
                value: "A".into(),
            })],
        };
        let bytes = encode_version(&d, Version::Ac12).unwrap();
        assert!(bytes.starts_with(b"AC1.2\0"));
        assert_eq!(
            u32::from_le_bytes(bytes[0x24..0x28].try_into().unwrap()) as usize,
            Version::Ac12.entity_start() + 4 + 32 + 3
        );
        let decoded = crate::parse(&bytes).unwrap();
        let Entity::OnLayer { entity, .. } = decoded.entities().next().unwrap() else {
            panic!("expected layer metadata")
        };
        let Entity::Text { height, .. } = entity.as_ref() else {
            panic!("expected TEXT")
        };
        assert!((*height - 0.75).abs() < 1e-12);
    }
}
