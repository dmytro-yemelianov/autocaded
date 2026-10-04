//! Synthetic static-contract fixtures, not original-generated OFF exports.
use acad_dxf::{parse, try_write, write, DxfError};
fn drawing() -> acad_model::Drawing {
    let mut d = parse(b"POINT,1\r\n1,2\r\n").unwrap();
    d.header.current_layer = 7;
    d.header.layers = [(0, 0), (1, 1), (7, 7), (64, 64), (127, 127), (2, 254)]
        .into_iter()
        .collect();
    d.header.off_layers = [1, 7, 64, 127].into_iter().collect();
    d
}
fn table(bytes: &[u8]) -> Vec<i16> {
    let text = std::str::from_utf8(bytes).unwrap();
    text.split("LAYERC,1\r\n")
        .nth(1)
        .unwrap()
        .lines()
        .take(8)
        .flat_map(|row| row.split(',').map(|n| n.parse().unwrap()))
        .collect()
}
#[test]
fn signed_decimal_off_table_and_legacy_writer_roundtrip() {
    let d = drawing();
    let bytes = try_write(&d).unwrap();
    assert_eq!(bytes, write(&d));
    let slots = table(&bytes);
    assert_eq!(slots.len(), 128);
    for (layer, value) in [
        (0, 0),
        (1, -1),
        (7, -7),
        (64, -64),
        (127, -127),
        (2, 254),
        (3, 255),
    ] {
        assert_eq!(slots[layer], value);
    }
    let reopened = parse(&bytes).unwrap();
    assert_eq!(reopened.header.layers, d.header.layers);
    assert_eq!(reopened.header.off_layers, d.header.off_layers);
    assert_eq!(reopened.header.current_layer, 7);
    assert_eq!(reopened.items, d.items);
    assert!(!reopened.header.layer_is_visible(7));
    assert!(reopened.header.layer_is_visible(2));
    assert_eq!(try_write(&reopened).unwrap(), bytes);
}
#[test]
fn every_positive_color_and_unused_slot_preserves_prior_semantics() {
    for color in 0..=254 {
        let mut d = drawing();
        d.header.off_layers.clear();
        d.header.layers.insert(7, color);
        let reopened = parse(&try_write(&d).unwrap()).unwrap();
        assert_eq!(reopened.header.layers[&7], color);
        assert!(!reopened.header.layers.contains_key(&3));
        assert!(reopened.header.off_layers.is_empty());
    }
}
#[test]
fn invalid_signed_values_and_native_off_edges_are_checked() {
    let d = drawing();
    for (layer, value) in [(0, -7i16), (7, -128), (7, -254), (7, -255), (7, i16::MIN)] {
        let mut slots = table(&try_write(&d).unwrap());
        slots[layer] = value;
        let mut text = String::from("LAYERC,1\r\n");
        for row in slots.chunks(16) {
            text.push_str(&row.iter().map(i16::to_string).collect::<Vec<_>>().join(","));
            text.push_str("\r\n");
        }
        assert_eq!(
            parse(text.as_bytes()).unwrap_err(),
            DxfError::InvalidLayerColor {
                layer: layer as u8,
                value
            }
        );
    }
    for (layer, color) in [
        (0, Some(7)),
        (1, Some(0)),
        (1, Some(128)),
        (1, Some(254)),
        (3, None),
        (128, None),
        (255, None),
    ] {
        let mut invalid = d.clone();
        invalid.header.off_layers.clear();
        invalid.header.off_layers.insert(layer);
        if let Some(color) = color {
            invalid.header.layers.insert(layer, color);
        }
        assert_eq!(
            try_write(&invalid).unwrap_err(),
            DxfError::InvalidLayerVisibility { layer, color }
        );
        assert!(std::panic::catch_unwind(|| write(&invalid)).is_err());
    }
}
#[test]
fn later_layer_table_replaces_off_and_unused_slots() {
    let d = drawing();
    let mut text = String::from_utf8(try_write(&d).unwrap()).unwrap();
    text = text.trim_end_matches(char::from(26)).to_owned();
    text.push_str("LAYERC,1\r\n");
    for _ in 0..8 {
        text.push_str(&vec!["255"; 16].join(","));
        text.push_str("\r\n");
    }
    let reopened = parse(text.as_bytes()).unwrap();
    assert!(reopened.header.layers.is_empty());
    assert!(reopened.header.off_layers.is_empty());
}
