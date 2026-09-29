#[cfg(unix)]
#[test]
fn original_creates_shapes_from_a_loaded_library() {
    use acad_model::{Entity, Item, Point};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/raw/Autodesk AutoCAD 1.4 (5.25)");
    let system = root.join("System.img");
    let samples = root.join("Samples.img");
    if !system.exists() || !samples.exists() || !acad_oracle::available() {
        eprintln!("skipping oracle: extracted floppy images or qemu-system-i386 absent");
        return;
    }
    let (dwg, dxf) = acad_oracle::generate_pair_with_samples(
        &system,
        &samples,
        "ORCSHAPE",
        &[
            "LOAD", "B:ES", "SHAPE", "RES", "2.25,3.5", "0.75", "30", "SHAPE", "CAP", "6.5,2.75",
            "1.25", "75",
        ],
    )
    .unwrap();
    let drawing = acad_dxf::parse(&dxf).unwrap();
    // IDs come from ES.SHP's definition headers; geometry comes from the
    // commands above, independently of either codec.
    assert_eq!(
        drawing.items,
        vec![
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Load {
                    name: "B:ES".into()
                })
            }),
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Shape {
                    origin: Point { x: 2.25, y: 3.5 },
                    height: 0.75,
                    rotation_deg: 30.0,
                    number: 129
                })
            }),
            Item::Entity(Entity::OnLayer {
                layer: 1,
                entity: Box::new(Entity::Shape {
                    origin: Point { x: 6.5, y: 2.75 },
                    height: 1.25,
                    rotation_deg: 75.0,
                    number: 130
                })
            }),
        ]
    );
    let end = dxf.iter().position(|&b| b == 0x1a).unwrap() + 1;
    assert_eq!(acad_dxf::write(&drawing), dxf[..end]);
    assert_eq!(acad_dxf::write(&acad_dwg::parse(&dwg).unwrap()), dxf[..end]);
    let (_, meta) = acad_dwg::header::parse_header(&dwg).unwrap();
    for cut in 0..meta.entity_end as usize {
        assert!(
            acad_dwg::parse(&dwg[..cut]).is_err(),
            "accepted truncated LOAD/SHAPE at {cut:#x}"
        );
    }
}
