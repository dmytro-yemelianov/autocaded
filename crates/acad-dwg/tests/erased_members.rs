//! Constructed physical sign fixtures based on the retained per-record rule.
//! They do not assert original native whole-owner selection parity.
use acad_dwg::header::Version;
use acad_model::{Entity, Item};
fn record(bytes: &mut Vec<u8>, kind: i16, layer: u16, body: &[u8]) {
    bytes.extend(kind.to_le_bytes());
    bytes.extend(layer.to_le_bytes());
    bytes.extend(body);
}
fn numbers(values: &[f64]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn text(value: &str) -> Vec<u8> {
    let mut out = (value.len() as u16).to_le_bytes().to_vec();
    out.extend(value.as_bytes());
    out
}
fn end() -> Vec<u8> {
    let mut out = 2u16.to_le_bytes().to_vec();
    out.extend(1u16.to_le_bytes());
    out.extend(numbers(&[3.0, 0.0]));
    out
}
fn fixture(version: Version) -> (Vec<u8>, Vec<u8>) {
    let d = acad_dxf::parse(b"POINT,1\r\n0,0\r\n").unwrap();
    let mut bytes = acad_dwg::write_version(&d, version).unwrap();
    bytes.truncate(version.entity_start());
    let mut records = Vec::new();
    let mut block = text("B");
    block.extend(numbers(&[0.0, 0.0]));
    record(&mut records, 12, 1, &block);
    record(&mut records, 5, 7, &[]);
    record(&mut records, -1, 2, &numbers(&[-0.0, 2.0, 3.0, 4.0]));
    record(&mut records, 5, 8, &[]);
    record(&mut records, -2, 3, &numbers(&[5.0, 6.0]));
    record(&mut records, -3, 4, &numbers(&[7.0, 8.0, 9.0]));
    let mut shape = numbers(&[1.0, 2.0, 3.0, 0.0]);
    shape.extend(129u16.to_le_bytes());
    record(&mut records, -4, 5, &shape);
    let mut label = numbers(&[
        1.0,
        2.0,
        if version == Version::Ac12 { 4.0 } else { 3.0 },
        0.0,
    ]);
    label.extend(text("A"));
    record(&mut records, -7, 6, &label);
    record(&mut records, -8, 9, &numbers(&[1.0, 2.0, 3.0, 0.0, 0.0]));
    record(
        &mut records,
        -9,
        10,
        &numbers(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
    );
    record(&mut records, -10, 11, &text("ALT"));
    record(
        &mut records,
        -11,
        12,
        &numbers(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]),
    );
    record(&mut records, 6, 13, &end());
    record(&mut records, 2, 14, &numbers(&[10.0, 20.0]));
    record(&mut records, 6, 15, &end());
    record(&mut records, 13, 1, &[]);
    record(&mut records, 5, 16, &[]);
    let mut insert = text("B");
    insert.extend(numbers(&[30.0, 40.0, 1.0, 1.0, 0.0]));
    record(&mut records, -14, 17, &insert);
    record(&mut records, -10, 18, &text("ES"));
    record(&mut records, 2, 19, &numbers(&[50.0, 60.0]));
    record(&mut records, 6, 20, &end());
    bytes.extend(&records);
    let entity_end = bytes.len() as u32;
    bytes[0x24..0x28].copy_from_slice(&entity_end.to_le_bytes());
    bytes[0x28..0x2a].copy_from_slice(&21u16.to_le_bytes());
    (bytes, records)
}
#[test]
fn every_erased_ordinary_type_retains_sign_layer_fields_and_order_inside_live_groups() {
    for version in [Version::Ac12, Version::Ac140] {
        let (bytes, records) = fixture(version);
        let drawing = acad_dwg::parse(&bytes).unwrap();
        let block = drawing.block("B").unwrap();
        let Entity::Repeat(outer) = &block.entities[0] else {
            panic!("outer")
        };
        let Entity::Erased(line) = &outer.entities[0] else {
            panic!("erased LINE")
        };
        assert!(
            matches!(line.as_ref(), Entity::OnLayer { layer: 2, entity } if matches!(entity.as_ref(), Entity::Line { start, .. } if start.x.to_bits() == (-0.0f64).to_bits()))
        );
        let Entity::Repeat(inner) = &outer.entities[1] else {
            panic!("inner")
        };
        assert_eq!((inner.start_layer, inner.end_layer), (8, 13));
        for (member, layer) in inner.entities.iter().zip([3, 4, 5, 6, 9, 10, 11, 12]) {
            let Entity::Erased(member) = member else {
                panic!("erased ordinary member")
            };
            assert!(
                matches!(member.as_ref(), Entity::OnLayer { layer: actual, .. } if *actual == layer)
            );
        }
        let Item::Repeat(root) = &drawing.items[1] else {
            panic!("root group")
        };
        assert!(
            matches!(&root.entities[0], Entity::Erased(entity) if matches!(entity.as_ref(), Entity::OnLayer { layer: 17, entity } if matches!(entity.as_ref(), Entity::Insert { name, .. } if name == "B")))
        );
        assert_eq!(
            drawing.entities().count(),
            1,
            "erased root members do not enter live convenience traversal"
        );
        let meta = acad_dwg::header::parse_header(&bytes).unwrap().1;
        assert_eq!(
            acad_dwg::entity::read_entities(&bytes, &meta)
                .unwrap()
                .len(),
            2
        );
        let encoded = acad_dwg::write_version(&drawing, version).unwrap();
        assert_eq!(
            &encoded[version.entity_start()..meta.entity_end as usize],
            &records
        );
        let written = acad_dwg::header::parse_header(&encoded).unwrap().1;
        assert_eq!(
            (written.version, written.entity_count, written.entity_end),
            (version, 21, meta.entity_end),
            "physical count includes every negative member record"
        );
        assert_eq!(
            acad_dwg::write::encode_version(&drawing, version).unwrap(),
            encoded
        );
        assert_eq!(
            acad_dwg::write_version(&acad_dwg::parse(&encoded).unwrap(), version).unwrap(),
            encoded
        );
        let dxf = acad_dxf::try_write(&drawing).unwrap();
        let live = acad_dxf::parse(&dxf).unwrap();
        assert_eq!(acad_dxf::write(&drawing), dxf);
        let Entity::Repeat(live_outer) = &live.block("B").unwrap().entities[0] else {
            panic!("live outer")
        };
        assert_eq!(
            live_outer.entities.len(),
            1,
            "erased-only nested group pruned"
        );
        assert!(matches!(
            live_outer.entities[0],
            Entity::OnLayer { layer: 14, .. }
        ));
        let Item::Repeat(live_root) = &live.items[1] else {
            panic!("live root")
        };
        assert_eq!(live_root.entities.len(), 1);
        assert!(matches!(
            live_root.entities[0],
            Entity::OnLayer { layer: 19, .. }
        ));
        assert_eq!(acad_dxf::try_write(&live).unwrap(), dxf);
    }
}

#[test]
fn whole_owner_with_prior_erased_member_history_is_ambiguous_and_checked() {
    let mut drawing = acad_dxf::parse(b"REPEAT,7\r\nREPEAT,8\r\nPOINT,2\r\n1,2\r\nENDREP,9\r\n2,1,3,0\r\nENDREP,10\r\n2,1,4,0\r\n").unwrap();
    let Item::Repeat(mut outer) = drawing.items.remove(0) else {
        panic!("outer")
    };
    let Entity::Repeat(inner) = &mut outer.entities[0] else {
        panic!("inner")
    };
    inner.entities[0] = Entity::Erased(Box::new(inner.entities[0].clone()));
    drawing.items = vec![Item::Repeat(outer.clone())];
    for version in [Version::Ac12, Version::Ac140] {
        let bytes = acad_dwg::write_version(&drawing, version).unwrap();
        assert_eq!(acad_dwg::parse(&bytes).unwrap().items, drawing.items);
        drawing.items = vec![Item::Erased(Entity::Repeat(outer.clone()))];
        for result in [
            acad_dwg::write_version(&drawing, version),
            acad_dwg::write::encode_version(&drawing, version),
        ] {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("already erased members"));
        }
        drawing.items = vec![Item::Repeat(outer.clone())];
    }
    drawing.items = vec![Item::Erased(Entity::Repeat(outer))];
    assert!(acad_dwg::write(&drawing).is_err());
    assert!(acad_dwg::write::encode(&drawing).is_err());
    assert!(acad_dxf::parse(&acad_dxf::try_write(&drawing).unwrap())
        .unwrap()
        .items
        .is_empty());
}

#[test]
fn confirmed_original_form_keeps_markers_live_and_erases_every_member() {
    use acad_model::group_codec::{has_ambiguous_owner, original_member_erasure};
    let parsed = acad_dxf::parse(b"REPEAT,7\r\nPOINT,3\r\n0,1\r\nREPEAT,8\r\nPOINT,2\r\n1,2\r\nPOINT,4\r\n3,4\r\nENDREP,9\r\n2,1,3,0\r\nENDREP,10\r\n2,1,4,0\r\n").unwrap();
    let Item::Repeat(mut outer) = parsed.items[0].clone() else {
        panic!("outer")
    };
    let mut drawing = parsed.clone();
    // Unambiguous erased owner: unchanged native uniform-negative form.
    drawing.items = vec![Item::Erased(Entity::Repeat(outer.clone()))];
    assert!(!has_ambiguous_owner(&drawing));
    assert_eq!(original_member_erasure(&drawing), (drawing.clone(), 0));
    let Entity::Repeat(inner) = &mut outer.entities[1] else {
        panic!("inner")
    };
    let prior = inner.entities[0].clone();
    inner.entities[0] = Entity::Erased(Box::new(prior));
    drawing.items = vec![Item::Erased(Entity::Repeat(outer.clone()))];
    let before = drawing.clone();
    assert!(has_ambiguous_owner(&drawing));
    let (converted, count) = original_member_erasure(&drawing);
    assert_eq!(count, 1);
    assert_eq!(drawing, before);
    let Item::Repeat(group) = &converted.items[0] else {
        panic!("live markers")
    };
    assert_eq!((group.start_layer, group.end_layer), (7, 10));
    assert!(group.entities[0].is_erased());
    let Entity::Repeat(nested) = &group.entities[1] else {
        panic!("nested live markers")
    };
    assert_eq!((nested.start_layer, nested.end_layer), (8, 9));
    // The earlier erased member keeps a single tag, never a stacked one.
    assert_eq!(
        nested.entities[0],
        match &outer.entities[1] {
            Entity::Repeat(inner) => inner.entities[0].clone(),
            _ => unreachable!(),
        }
    );
    assert!(nested.entities.iter().all(Entity::is_erased));
    for version in [Version::Ac12, Version::Ac140] {
        let bytes = acad_dwg::write_version(&converted, version).unwrap();
        assert_eq!(acad_dwg::parse(&bytes).unwrap().items, converted.items);
        let (_, meta) = acad_dwg::header::parse_header(&bytes).unwrap();
        assert_eq!(meta.entity_count, 7);
        // Signed type words in physical order: +5, -2, +5, -2, -2, +6, +6.
        let mut pos = version.entity_start();
        let mut types = Vec::new();
        for size in [4, 4 + 16, 4, 4 + 16, 4 + 16, 4 + 20, 4 + 20] {
            types.push(i16::from_le_bytes([bytes[pos], bytes[pos + 1]]));
            pos += size;
        }
        assert_eq!(types, [5, -2, 5, -2, -2, 6, 6]);
        assert_eq!(pos, meta.entity_end as usize);
    }
    // A malformed explicit owner layer is left for the checked refusal.
    drawing.items = vec![Item::Erased(Entity::Repeat(acad_model::Repeat {
        entities: vec![
            Entity::OnLayer {
                layer: 2,
                entity: Box::new(Entity::Repeat(group.clone())),
            },
            Entity::Erased(Box::new(group.entities[0].clone())),
        ],
        ..group.clone()
    }))];
    let (converted, _) = original_member_erasure(&drawing);
    assert!(acad_dwg::write(&converted).is_err());
}

#[test]
fn ordinary_erasure_and_layer_wrapper_orders_preserve_one_physical_sign_and_layer() {
    let mut drawing =
        acad_dxf::parse(b"REPEAT,7\r\nPOINT,2\r\n1,2\r\nENDREP,9\r\n2,1,3,0\r\n").unwrap();
    let Item::Repeat(r) = &mut drawing.items[0] else {
        panic!("group")
    };
    let point = Entity::Point {
        origin: acad_model::Point { x: 1.0, y: 2.0 },
    };
    r.entities = vec![
        Entity::Erased(Box::new(Entity::OnLayer {
            layer: 2,
            entity: Box::new(point.clone()),
        })),
        Entity::OnLayer {
            layer: 2,
            entity: Box::new(Entity::Erased(Box::new(point))),
        },
    ];
    let bytes = acad_dwg::write(&drawing).unwrap();
    let reopened = acad_dwg::parse(&bytes).unwrap();
    let Item::Repeat(r) = &reopened.items[0] else {
        panic!("group")
    };
    assert_eq!(r.entities[0], r.entities[1]);
    assert_eq!(
        &bytes[Version::Ac140.entity_start() + 4..Version::Ac140.entity_start() + 24],
        &bytes[Version::Ac140.entity_start() + 24..Version::Ac140.entity_start() + 44]
    );
    assert_eq!(acad_dwg::write(&reopened).unwrap(), bytes);
}

#[test]
fn small_native_partial_member_fixture_for_gui_has_one_visible_line() {
    let mut drawing = acad_dxf::parse(b"LIMITS,1\r\n0,10,0,10\r\nDWGVIEW,1\r\n3,2,10\r\nREPEAT,1\r\nLINE,2\r\n0,0,0,4\r\nLINE,1\r\n1,0,5,0\r\nENDREP,1\r\n1,1,0,0\r\n").unwrap();
    let Item::Repeat(r) = &mut drawing.items[0] else {
        panic!("group")
    };
    r.entities[0] = Entity::Erased(Box::new(r.entities[0].clone()));
    let bytes = acad_dwg::write_version(&drawing, Version::Ac140).unwrap();
    let reopened = acad_dwg::parse(&bytes).unwrap();
    assert_eq!(reopened.items, drawing.items);
    assert_eq!(reopened.entities().count(), 1);
    assert_eq!(
        acad_dwg::header::parse_header(&bytes)
            .unwrap()
            .1
            .entity_count,
        4
    );
    let live = acad_dxf::parse(&acad_dxf::try_write(&drawing).unwrap()).unwrap();
    let Item::Repeat(live) = &live.items[0] else {
        panic!("live group")
    };
    assert_eq!(live.entities.len(), 1);
    assert!(
        matches!(&live.entities[0], Entity::OnLayer { layer: 1, entity } if matches!(entity.as_ref(), Entity::Line { start, end } if *start == acad_model::Point { x: 1.0, y: 0.0 } && *end == acad_model::Point { x: 5.0, y: 0.0 }))
    );
    if let Some(path) = std::env::var_os("B4_GUI_FIXTURE_PATH") {
        std::fs::write(path, bytes).unwrap();
    }
}

#[test]
fn erased_member_tag_at_root_is_nonlive_and_checked_refuses_category_drift() {
    let mut drawing = acad_dxf::parse(b"POINT,2\r\n1,2\r\n").unwrap();
    let Item::Entity(entity) = &mut drawing.items[0] else {
        panic!("point")
    };
    *entity = Entity::Erased(Box::new(entity.clone()));
    assert!(entity.is_erased());
    assert!(!drawing.header.entity_is_visible(entity));
    assert_eq!(drawing.entities().count(), 0);
    for version in [Version::Ac12, Version::Ac140] {
        assert!(acad_dwg::write_version(&drawing, version)
            .unwrap_err()
            .to_string()
            .contains("drawing root must use Item::Erased"));
    }
    assert!(acad_dxf::try_write(&drawing)
        .unwrap_err()
        .to_string()
        .contains("drawing root must use Item::Erased"));
    assert!(std::panic::catch_unwind(|| acad_dxf::write(&drawing)).is_err());
    let Item::Entity(Entity::Erased(inner)) = drawing.items.remove(0) else {
        panic!("member")
    };
    drawing.items.push(Item::Erased(*inner));
    for version in [Version::Ac12, Version::Ac140] {
        assert_eq!(
            acad_dwg::parse(&acad_dwg::write_version(&drawing, version).unwrap())
                .unwrap()
                .items,
            drawing.items
        );
    }
    assert!(acad_dxf::parse(&acad_dxf::try_write(&drawing).unwrap())
        .unwrap()
        .items
        .is_empty());
}

#[test]
fn erased_leaf_corruption_and_stored_record_limits_remain_checked() {
    for version in [Version::Ac12, Version::Ac140] {
        let (bytes, _) = fixture(version);
        // The first negative LINE follows BLOCK(string + base) and a start marker.
        let at = version.entity_start() + 4 + 3 + 16 + 4;
        assert_eq!(
            i16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()),
            -1
        );
        let mut invalid_layer = bytes.clone();
        invalid_layer[at + 2..at + 4].copy_from_slice(&256u16.to_le_bytes());
        assert!(matches!(
            acad_dwg::parse(&invalid_layer),
            Err(acad_dwg::DwgError::InvalidEntityLayer { .. })
        ));
        let mut nonfinite = bytes.clone();
        nonfinite[at + 4..at + 12].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(acad_dwg::parse(&nonfinite).is_err());
        let mut truncated = bytes;
        truncated.truncate(at + 10);
        assert!(acad_dwg::parse(&truncated).is_err());
    }
    let mut d = acad_dxf::parse(b"REPEAT,1\r\nPOINT,2\r\n1,2\r\nENDREP,1\r\n1,1,0,0\r\n").unwrap();
    let Item::Repeat(r) = &mut d.items[0] else {
        panic!("group")
    };
    r.entities = vec![Entity::Erased(Box::new(r.entities[0].clone())); 65534];
    assert!(acad_dwg::write(&d)
        .unwrap_err()
        .to_string()
        .contains("record count"));
    assert!(
        acad_dxf::try_write(&d)
            .unwrap_err()
            .to_string()
            .contains("record count"),
        "omitted members cannot bypass stored preflight"
    );
}
