use acad_app::{
    art::Primitive,
    art_image::{convert, recipe_bytes, Config},
};
use sha2::{Digest, Sha256};
fn png(width: u32, height: u32, kind: png::ColorType, data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(kind);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(data)
            .unwrap();
    }
    bytes
}
fn config(bytes: &[u8]) -> Config {
    Config {
        schema_version: 1,
        id: "test-image".into(),
        image: "input.png".into(),
        image_sha256: format!("{:x}", Sha256::digest(bytes)),
        columns: 4,
        colors: 3,
        contours: false,
        contour_color: 250,
        contrast_threshold: 0,
        matte: [255; 3],
    }
}
#[test]
fn strips_merge_into_editable_regions_preserve_y_orientation_and_codecs() {
    let mut pixels = Vec::new();
    for y in 0..8 {
        for _ in 0..8 {
            pixels.extend(if y < 4 { [255, 0, 0] } else { [0, 0, 255] });
        }
    }
    let bytes = png(8, 8, png::ColorType::Rgb, &pixels);
    let (recipe, report) = convert(&config(&bytes), &bytes).unwrap();
    assert_eq!((report.regions, report.contours), (2, 0));
    assert_eq!(report.palette, vec![1, 5]);
    let Primitive::Rectangle {
        layer,
        min,
        max,
        filled,
    } = recipe.primitives[0]
    else {
        panic!()
    };
    assert_eq!(
        (layer, min, max, filled),
        (1, [0.0, 50.0], [100.0, 100.0], true)
    );
    let drawing = recipe.compile().unwrap();
    assert_eq!(
        acad_dwg::parse(&acad_dwg::write(&drawing).unwrap())
            .unwrap()
            .items,
        drawing.items
    );
    assert_eq!(
        acad_dxf::parse(&acad_dxf::try_write(&drawing).unwrap())
            .unwrap()
            .items,
        drawing.items
    );
    assert_eq!(
        recipe_bytes(&recipe).unwrap(),
        recipe_bytes(&convert(&config(&bytes), &bytes).unwrap().0).unwrap()
    );
}
#[test]
fn exact_cell_coverage_with_no_overlaps_or_holes_and_merged_contours() {
    let pixels: Vec<_> = (0..16)
        .flat_map(|i| {
            if (i / 4 + i % 4) % 2 == 0 {
                [255, 0, 0]
            } else {
                [0, 0, 255]
            }
        })
        .collect();
    let bytes = png(4, 4, png::ColorType::Rgb, &pixels);
    let mut cfg = config(&bytes);
    cfg.contours = true;
    let (r, report) = convert(&cfg, &bytes).unwrap();
    assert_eq!((report.regions, report.contours), (16, 10));
    let mut covered = [0; 16];
    for primitive in &r.primitives {
        if let Primitive::Rectangle { min, max, .. } = primitive {
            for y in 0..4 {
                for x in 0..4 {
                    let p = [(x as f64 + 0.5) * 25.0, (y as f64 + 0.5) * 25.0];
                    if p[0] >= min[0] && p[0] < max[0] && p[1] >= min[1] && p[1] < max[1] {
                        covered[y * 4 + x] += 1;
                    }
                }
            }
        }
    }
    assert_eq!(covered, [1; 16]);
    cfg.contrast_threshold = 442;
    assert_eq!(convert(&cfg, &bytes).unwrap().1.contours, 4); // exterior only
}
#[test]
fn alpha_uses_explicit_matte_and_subcell_sources_do_not_divide_by_zero() {
    let bytes = png(1, 1, png::ColorType::Rgba, &[255, 0, 0, 0]);
    let cfg = config(&bytes);
    let (r, report) = convert(&cfg, &bytes).unwrap();
    assert_eq!((report.columns, report.rows), (1, 1));
    assert_eq!(report.palette, vec![7]);
    r.compile().unwrap();
    let bytes = png(1, 1, png::ColorType::GrayscaleAlpha, &[30, 0]);
    let mut cfg = config(&bytes);
    cfg.matte = [0; 3];
    assert_eq!(convert(&cfg, &bytes).unwrap().1.palette, vec![250]);
}
#[test]
fn rejects_wrong_hash_unknown_settings_malformed_png_and_excessive_aspect() {
    let bytes = png(4, 4, png::ColorType::Grayscale, &[127; 16]);
    let mut cfg = config(&bytes);
    cfg.image_sha256 = "0".repeat(64);
    assert!(convert(&cfg, &bytes).unwrap_err().contains("SHA-256"));
    cfg = config(&bytes);
    cfg.colors = 16;
    assert!(convert(&cfg, &bytes).is_err());
    cfg = config(&bytes);
    cfg.columns = 0;
    assert!(convert(&cfg, &bytes).is_err());
    let mut json = serde_json::to_value(config(&bytes)).unwrap();
    json["arbitrary_command"] = serde_json::json!("LINE");
    assert!(serde_json::from_value::<Config>(json).is_err());
    let bytes = b"not PNG";
    assert!(convert(&config(bytes), bytes).is_err());
    let bytes = png(4, 132, png::ColorType::Grayscale, &[0; 4 * 132]);
    assert!(convert(&config(&bytes), &bytes)
        .unwrap_err()
        .contains("128 rows"));
}
#[test]
fn fractional_source_aspect_obeys_historical_coordinate_rounding() {
    let bytes = png(7, 9, png::ColorType::Rgb, &[200; 7 * 9 * 3]);
    let (r, _) = convert(&config(&bytes), &bytes).unwrap();
    r.compile().unwrap();
    assert_eq!(r.bounds[3], 128.571429);
}
#[test]
fn committed_conversions_reproduce_inputs_settings_and_recipe_bytes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demo/art");
    for id in ["self-portrait", "chrysler"] {
        let cfg: Config = serde_json::from_slice(
            &std::fs::read(root.join(format!("sources/{id}-conversion.json"))).unwrap(),
        )
        .unwrap();
        let image = std::fs::read(root.join("sources").join(&cfg.image)).unwrap();
        let source: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(format!("sources/{id}.json"))).unwrap(),
        )
        .unwrap();
        let original = std::fs::read(
            root.join("sources")
                .join(source["image_file"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(
            source["image_sha256"].as_str().unwrap(),
            format!("{:x}", Sha256::digest(&original))
        );
        assert_eq!(source["input_sha256"].as_str().unwrap(), cfg.image_sha256);

        let (recipe, report) = convert(&cfg, &image).unwrap();
        assert!(report.regions > 1 && report.regions + report.contours <= 5000);
        assert_eq!(
            recipe_bytes(&recipe).unwrap(),
            std::fs::read(root.join(format!("recipes/{id}.json"))).unwrap()
        );
    }
}

#[test]
fn complexity_and_decoder_dimensions_are_bounded() {
    let pixels: Vec<_> = (0..128 * 128)
        .flat_map(|i| {
            if (i / 128 + i % 128) % 2 == 0 {
                [255, 0, 0]
            } else {
                [0, 0, 255]
            }
        })
        .collect();
    let bytes = png(128, 128, png::ColorType::Rgb, &pixels);
    let mut cfg = config(&bytes);
    cfg.columns = 128;
    cfg.colors = 2;
    assert!(convert(&cfg, &bytes).unwrap_err().contains("5000 entities"));
    let bytes = png(4097, 1, png::ColorType::Grayscale, &[0; 4097]);
    assert!(convert(&config(&bytes), &bytes)
        .unwrap_err()
        .contains("4096-axis"));
    let bytes = vec![0; 16 * 1024 * 1024 + 1];
    assert!(convert(&config(&bytes), &bytes)
        .unwrap_err()
        .contains("16 MiB"));
}

#[test]
fn animated_png_is_rejected_instead_of_silently_selecting_a_frame() {
    let mut bytes = Vec::new();
    {
        let mut e = png::Encoder::new(&mut bytes, 4, 4);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.set_animated(2, 0).unwrap();
        let mut w = e.write_header().unwrap();
        w.write_image_data(&[100; 48]).unwrap();
        w.write_image_data(&[200; 48]).unwrap();
    }
    assert!(convert(&config(&bytes), &bytes)
        .unwrap_err()
        .contains("animated PNG"));
}
