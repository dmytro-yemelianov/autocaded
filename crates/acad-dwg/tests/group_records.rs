//! Independent physical fixtures; no translated runtime or original editor parity claim.
use acad_dwg::{
    header::{HeaderMeta, Version},
    DwgError,
};
use acad_model::{Entity, Item};
fn marker(out: &mut Vec<u8>, kind: i16, layer: u16) {
    out.extend(kind.to_le_bytes());
    out.extend(layer.to_le_bytes());
}
fn point(out: &mut Vec<u8>, layer: u16) {
    marker(out, 2, layer);
    out.extend(1.25f64.to_le_bytes());
    out.extend(2.5f64.to_le_bytes());
}
fn close(out: &mut Vec<u8>, layer: u16) {
    marker(out, 6, layer);
    out.extend(3u16.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend((-10.0f64).to_le_bytes());
    out.extend(5.0f64.to_le_bytes());
}
fn fixture(records: &[u8], count: u32, version: Version) -> (Vec<u8>, HeaderMeta) {
    let drawing = acad_dxf::parse(b"POINT,1\r\n0,0\r\n").unwrap();
    let mut bytes = acad_dwg::write_version(&drawing, version).unwrap();
    bytes.truncate(version.entity_start());
    bytes.extend(records);
    let meta = HeaderMeta {
        version,
        entity_count: count,
        entity_end: bytes.len() as u32,
    };
    bytes[0x24..0x28].copy_from_slice(&meta.entity_end.to_le_bytes());
    bytes[0x28..0x2a].copy_from_slice(&(count as u16).to_le_bytes());
    (bytes, meta)
}
#[test]
fn independent_headers_preserve_marker_child_and_closing_layers_in_both_revisions() {
    let mut records = Vec::new();
    marker(&mut records, 5, 7);
    point(&mut records, 3);
    marker(&mut records, 5, 4);
    marker(&mut records, 10, 6);
    records.extend(4u16.to_le_bytes());
    records.extend(b"FONT");
    point(&mut records, 5);
    close(&mut records, 8);
    close(&mut records, 9);
    for version in [Version::Ac12, Version::Ac140] {
        let (bytes, meta) = fixture(&records, 7, version);
        let items = acad_dwg::entity::read_items(&bytes, &meta).unwrap();
        let Item::Repeat(outer) = &items[0] else {
            panic!("outer pattern");
        };
        assert_eq!((outer.start_layer, outer.end_layer), (7, 9));
        assert!(matches!(
            &outer.entities[0],
            Entity::OnLayer { layer: 3, .. }
        ));
        let Entity::Repeat(inner) = &outer.entities[1] else {
            panic!("nested pattern");
        };
        assert_eq!((inner.start_layer, inner.end_layer), (4, 8));
        assert!(
            matches!(&inner.entities[0], Entity::OnLayer { layer: 6, entity } if matches!(entity.as_ref(), Entity::Load { name } if name == "FONT"))
        );
        assert!(matches!(
            &inner.entities[1],
            Entity::OnLayer { layer: 5, .. }
        ));
        assert_eq!(
            acad_dwg::entity::read_entities(&bytes, &meta)
                .unwrap()
                .len(),
            3
        );
        let drawing = acad_dwg::parse(&bytes).unwrap();
        let encoded = acad_dwg::write_version(&drawing, version).unwrap();
        assert_eq!(
            &encoded[version.entity_start()..meta.entity_end as usize],
            &records
        );
        assert_eq!(acad_dwg::parse(&encoded).unwrap().items, items);
        let dxf = acad_dxf::try_write(&drawing).unwrap();
        assert_eq!(acad_dxf::parse(&dxf).unwrap().items, items);
    }
}
#[test]
fn unbalanced_signed_structures_refuse_and_partial_erased_members_are_retained() {
    for version in [Version::Ac12, Version::Ac140] {
        for kind in [-5, -6] {
            let mut records = Vec::new();
            if kind == -5 {
                marker(&mut records, kind, 1);
            } else {
                close(&mut records, 1);
                records[..2].copy_from_slice(&kind.to_le_bytes());
            }
            let (bytes, meta) = fixture(&records, 1, version);
            assert!(acad_dwg::entity::read_items(&bytes, &meta).is_err());
            assert!(acad_dwg::entity::read_entities(&bytes, &meta).is_err());
        }
        let mut records = Vec::new();
        marker(&mut records, 5, 1);
        let offset = records.len();
        point(&mut records, 2);
        records[offset..offset + 2].copy_from_slice(&(-2i16).to_le_bytes());
        close(&mut records, 1);
        let (bytes, meta) = fixture(&records, 3, version);
        let items = acad_dwg::entity::read_items(&bytes, &meta).unwrap();
        let Item::Repeat(r) = &items[0] else {
            panic!("live group")
        };
        assert!(
            matches!(&r.entities[0], Entity::Erased(entity) if matches!(entity.as_ref(), Entity::OnLayer { layer: 2, entity } if matches!(entity.as_ref(), Entity::Point { origin } if *origin == acad_model::Point { x: 1.25, y: 2.5 })))
        );
        assert!(acad_dwg::entity::read_entities(&bytes, &meta)
            .unwrap()
            .is_empty());
        let drawing = acad_dwg::parse(&bytes).unwrap();
        let encoded = acad_dwg::write_version(&drawing, version).unwrap();
        assert_eq!(
            &encoded[version.entity_start()..meta.entity_end as usize],
            &records
        );
    }
}
#[test]
fn malformed_boundaries_truncation_and_depth_have_bounded_errors() {
    for version in [Version::Ac12, Version::Ac140] {
        let mut deep = Vec::new();
        for _ in 0..65 {
            marker(&mut deep, 5, 1);
        }
        let (bytes, meta) = fixture(&deep, 65, version);
        assert!(matches!(
            acad_dwg::entity::read_items(&bytes, &meta),
            Err(DwgError::ReadLimit { .. })
        ));
        let mut records = Vec::new();
        marker(&mut records, 5, 1);
        point(&mut records, 1);
        marker(&mut records, 13, 1);
        let (bytes, meta) = fixture(&records, 3, version);
        assert!(matches!(
            acad_dwg::entity::read_items(&bytes, &meta),
            Err(DwgError::InvalidGroupStream { .. })
        ));
        let mut records = Vec::new();
        marker(&mut records, 5, 1);
        point(&mut records, 1);
        close(&mut records, 1);
        for cut in 1..24 {
            let (mut bytes, mut meta) = fixture(&records, 3, version);
            bytes.truncate(bytes.len() - cut);
            meta.entity_end = bytes.len() as u32;
            assert!(acad_dwg::entity::read_items(&bytes, &meta).is_err());
        }
        let (bytes, meta) = fixture(&[], 0, version);
        assert!(matches!(
            acad_dwg::entity::read_items(
                &bytes,
                &HeaderMeta {
                    entity_count: 65536,
                    ..meta
                }
            ),
            Err(DwgError::ReadLimit { .. })
        ));
    }
}
#[test]
fn original_flat_marker_spans_round_trip_byte_for_byte_offline() {
    let Some(base) = corpus("Samples") else {
        return;
    };
    for name in ["FLOOR", "BLIVET"] {
        let bytes = std::fs::read(base.join(format!("{name}.DWG"))).unwrap();
        let drawing = acad_dwg::parse(&bytes).unwrap();
        let original = acad_dwg::header::parse_header(&bytes).unwrap().1;
        let encoded = acad_dwg::write_version(&drawing, original.version).unwrap();
        let end = original.entity_end as usize;
        // Exact physical original REPEAT/ENDREP spans (including child headers).
        for (start, length) in if name == "FLOOR" {
            vec![(0x16c9, 4 + 4 * 36 + 24), (0x1775, 4 + 4 * 36 + 24)]
        } else {
            vec![
                (0x3b4, 4 + 36 + 24),
                (0x3f4, 4 + 36 + 24),
                (0x434, 4 + 36 + 24),
            ]
        } {
            assert!(start + length <= end);
            assert_eq!(
                &encoded[start..start + length],
                &bytes[start..start + length],
                "{name} span {start:#x}"
            );
        }
    }
}

#[test]
fn all_negative_native_owner_retains_nested_records_and_rejects_every_mixed_sign() {
    let mut records = Vec::new();
    marker(&mut records, -5, 7);
    point(&mut records, 3);
    records[4..6].copy_from_slice(&(-2i16).to_le_bytes());
    marker(&mut records, -5, 4);
    marker(&mut records, -10, 6);
    records.extend(4u16.to_le_bytes());
    records.extend(b"FONT");
    point(&mut records, 5);
    records[38..40].copy_from_slice(&(-2i16).to_le_bytes());
    close(&mut records, 8);
    records[58..60].copy_from_slice(&(-6i16).to_le_bytes());
    close(&mut records, 9);
    records[82..84].copy_from_slice(&(-6i16).to_le_bytes());
    for version in [Version::Ac12, Version::Ac140] {
        let (bytes, meta) = fixture(&records, 7, version);
        let items = acad_dwg::entity::read_items(&bytes, &meta).unwrap();
        let Item::Erased(Entity::Repeat(outer)) = &items[0] else {
            panic!("erased owner")
        };
        assert_eq!((outer.start_layer, outer.end_layer), (7, 9));
        let Entity::Repeat(inner) = &outer.entities[1] else {
            panic!("nested pattern")
        };
        assert_eq!((inner.start_layer, inner.end_layer), (4, 8));
        assert!(
            matches!(&inner.entities[0], Entity::OnLayer { layer: 6, entity } if matches!(entity.as_ref(), Entity::Load { name } if name == "FONT"))
        );
        assert!(acad_dwg::entity::read_entities(&bytes, &meta)
            .unwrap()
            .is_empty());
        let drawing = acad_dwg::parse(&bytes).unwrap();
        let encoded = acad_dwg::write_version(&drawing, version).unwrap();
        assert_eq!(
            &encoded[version.entity_start()..meta.entity_end as usize],
            &records
        );
        assert_eq!(acad_dwg::parse(&encoded).unwrap().items, items);
        assert!(acad_dxf::parse(&acad_dxf::try_write(&drawing).unwrap())
            .unwrap()
            .items
            .is_empty());
        assert_eq!(
            acad_dxf::write(&drawing),
            acad_dxf::try_write(&drawing).unwrap()
        );
        assert_eq!(
            acad_dwg::write::encode_version(&drawing, version).unwrap(),
            encoded
        );
        for at in [0, 4, 24, 28, 38, 58, 82] {
            let mut mixed = records.clone();
            let kind = i16::from_le_bytes(mixed[at..at + 2].try_into().unwrap());
            mixed[at..at + 2].copy_from_slice(&(-kind).to_le_bytes());
            let (bytes, meta) = fixture(&mixed, 7, version);
            assert!(
                acad_dwg::entity::read_items(&bytes, &meta).is_err(),
                "mixed sign at {at}"
            );
            assert!(
                acad_dwg::entity::read_entities(&bytes, &meta).is_err(),
                "geometry view at {at}"
            );
        }
        for at in [2, 6, 26, 30, 40, 60, 84] {
            let mut invalid = records.clone();
            invalid[at..at + 2].copy_from_slice(&256u16.to_le_bytes());
            let (bytes, meta) = fixture(&invalid, 7, version);
            assert!(matches!(
                acad_dwg::entity::read_items(&bytes, &meta),
                Err(DwgError::InvalidEntityLayer { .. })
            ));
        }
        for cut in 1..24 {
            let (mut bytes, mut meta) = fixture(&records, 7, version);
            bytes.truncate(bytes.len() - cut);
            meta.entity_end = bytes.len() as u32;
            assert!(acad_dwg::entity::read_items(&bytes, &meta).is_err());
        }
        for at in [62, 86] {
            let mut invalid = records.clone();
            invalid[at..at + 2].copy_from_slice(&0u16.to_le_bytes());
            let (bytes, meta) = fixture(&invalid, 7, version);
            assert!(acad_dwg::entity::read_items(&bytes, &meta).is_err());
        }
        let mut invalid = records.clone();
        invalid[66..74].copy_from_slice(&f64::NAN.to_le_bytes());
        let (bytes, meta) = fixture(&invalid, 7, version);
        assert!(acad_dwg::entity::read_items(&bytes, &meta).is_err());
        let (bytes, mut meta) = fixture(&records, 7, version);
        meta.entity_count = 6;
        assert!(matches!(
            acad_dwg::entity::read_items(&bytes, &meta),
            Err(DwgError::EntityCountMismatch { .. })
        ));
    }
}

#[test]
fn erased_subgroup_in_live_block_or_repeat_is_explicitly_unsupported() {
    for in_block in [false, true] {
        let mut records = Vec::new();
        if in_block {
            marker(&mut records, 12, 1);
            records.extend(1u16.to_le_bytes());
            records.extend(b"B");
            records.extend(0f64.to_le_bytes());
            records.extend(0f64.to_le_bytes());
        } else {
            marker(&mut records, 5, 1);
        }
        marker(&mut records, -5, 2);
        point(&mut records, 3);
        close(&mut records, 4);
        if in_block {
            marker(&mut records, 13, 1);
        } else {
            close(&mut records, 1);
        }
        let (bytes, meta) = fixture(&records, 5, Version::Ac140);
        assert!(matches!(
            acad_dwg::entity::read_items(&bytes, &meta),
            Err(DwgError::InvalidGroupStream { .. })
        ));
    }
    let mut records = Vec::new();
    for _ in 0..65 {
        marker(&mut records, -5, 1);
    }
    let (bytes, meta) = fixture(&records, 65, Version::Ac140);
    assert!(matches!(
        acad_dwg::entity::read_items(&bytes, &meta),
        Err(DwgError::ReadLimit { .. })
    ));
}

#[test]
fn signed_fields_unknown_types_and_zero_dimension_empty_groups_get_checked_errors() {
    for version in [Version::Ac12, Version::Ac140] {
        for sign in [1i16, -1] {
            let mut records = Vec::new();
            marker(&mut records, 5 * sign, 2);
            point(&mut records, 3);
            close(&mut records, 4);
            records[4..6].copy_from_slice(&(2 * sign).to_le_bytes());
            records[24..26].copy_from_slice(&(6 * sign).to_le_bytes());
            records[8..16].copy_from_slice(&f64::INFINITY.to_le_bytes());
            let (bytes, meta) = fixture(&records, 3, version);
            assert!(matches!(
                acad_dwg::entity::read_items(&bytes, &meta),
                Err(DwgError::NonFiniteEntity { .. })
            ));
            let mut empty = Vec::new();
            marker(&mut empty, 5 * sign, 2);
            close(&mut empty, 4);
            empty[4..6].copy_from_slice(&(6 * sign).to_le_bytes());
            // R6: a group without members is AutoCAD 1.4's own empty pair,
            // kept with its marker layers; zero dimensions stay refused.
            let (bytes, meta) = fixture(&empty, 2, version);
            let group = acad_model::Repeat {
                start_layer: 2,
                end_layer: 4,
                entities: Vec::new(),
                columns: 3,
                rows: 2,
                column_spacing: -10.0,
                row_spacing: 5.0,
            };
            assert_eq!(
                acad_dwg::entity::read_items(&bytes, &meta).unwrap(),
                [if sign > 0 {
                    Item::Repeat(group)
                } else {
                    Item::Erased(Entity::Repeat(group))
                }]
            );
            empty[8..10].copy_from_slice(&0u16.to_le_bytes());
            let (bytes, meta) = fixture(&empty, 2, version);
            assert!(matches!(
                acad_dwg::entity::read_items(&bytes, &meta),
                Err(DwgError::InvalidGroupStream { .. })
            ));
        }
        let mut records = Vec::new();
        marker(&mut records, i16::MIN, 1);
        let (bytes, meta) = fixture(&records, 1, version);
        assert!(matches!(
            acad_dwg::entity::read_items(&bytes, &meta),
            Err(DwgError::UnknownEntityType { code: 32768, .. })
        ));
    }
    for erased in [false, true] {
        let mut drawing = acad_dxf::parse(b"POINT,1\r\n0,0\r\n").unwrap();
        let point = Entity::Point {
            origin: acad_model::Point {
                x: f64::NAN,
                y: 0.0,
            },
        };
        drawing.items = vec![if erased {
            Item::Erased(point)
        } else {
            Item::Entity(point)
        }];
        assert!(acad_dwg::write(&drawing).is_err());
        assert!(acad_dxf::try_write(&drawing).is_err());
    }
}

#[test]
fn ac12_text_height_scaling_cannot_write_nonfinite_live_or_erased_fields() {
    for erased in [false, true] {
        let mut drawing = acad_dxf::parse(b"POINT,1\r\n0,0\r\n").unwrap();
        let text = Entity::Text {
            origin: acad_model::Point { x: 0.0, y: 0.0 },
            height: f64::MAX,
            rotation_deg: 0.0,
            value: "A".into(),
        };
        drawing.items = vec![if erased {
            Item::Erased(text)
        } else {
            Item::Entity(text)
        }];
        assert!(acad_dwg::write_version(&drawing, Version::Ac12).is_err());
        let bytes = acad_dwg::write_version(&drawing, Version::Ac140).unwrap();
        assert!(acad_dwg::parse(&bytes).is_ok());
    }
}

#[test]
fn stored_record_ceiling_applies_before_writing_live_and_erased_owners() {
    for erased in [false, true] {
        let mut drawing = acad_dxf::parse(b"POINT,1\r\n0,0\r\n").unwrap();
        let r = acad_model::Repeat {
            start_layer: 2,
            end_layer: 4,
            entities: vec![
                Entity::Point {
                    origin: acad_model::Point { x: 0.0, y: 0.0 }
                };
                65533
            ],
            columns: u16::MAX,
            rows: u16::MAX,
            column_spacing: 1.0,
            row_spacing: 1.0,
        };
        drawing.items = vec![if erased {
            Item::Erased(Entity::Repeat(r))
        } else {
            Item::Repeat(r)
        }];
        let bytes = acad_dwg::write(&drawing).unwrap();
        assert_eq!(
            acad_dwg::header::parse_header(&bytes)
                .unwrap()
                .1
                .entity_count,
            65535
        );
        let parsed = acad_dwg::parse(&bytes).unwrap();
        assert_eq!(acad_dwg::write(&parsed).unwrap(), bytes);
        assert!(acad_dxf::try_write(&drawing).is_ok());
        let r = match &mut drawing.items[0] {
            Item::Repeat(r) | Item::Erased(Entity::Repeat(r)) => r,
            _ => unreachable!(),
        };
        r.entities.push(Entity::Point {
            origin: acad_model::Point { x: 0.0, y: 0.0 },
        });
        assert!(acad_dwg::write(&drawing)
            .unwrap_err()
            .to_string()
            .contains("65535"));
        assert!(acad_dxf::try_write(&drawing)
            .unwrap_err()
            .to_string()
            .contains("65535"));
    }
}

/// `corpus/<path>` when the retained corpus is extracted. Otherwise the
/// caller skips visibly; `AUTOCAD_REQUIRE_CORPUS=1` makes absence a failure.
fn corpus(path: &str) -> Option<std::path::PathBuf> {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(path);
    if full.exists() {
        return Some(full);
    }
    let message = format!("corpus {} absent", full.display());
    assert!(
        std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
        "{message}"
    );
    eprintln!("skipping corpus test, NOT validated: {message}");
    None
}
