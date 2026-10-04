use acad_dxf::write;
use acad_model::{CustomEntity, Entity, Extents, GenericEntity, Item, Point};


#[derive(Debug, Clone)]
struct CustomPolyline {
    points: Vec<Point>,
    layer: u8,
}

impl CustomEntity for CustomPolyline {
    fn type_name(&self) -> &str {
        "POLYLINE"
    }
    fn layer(&self) -> u8 {
        self.layer
    }
    fn bounding_extents(&self) -> Option<Extents> {
        Extents::from_points(&self.points)
    }
    fn tessellate_points(&self) -> Vec<Vec<Point>> {
        vec![self.points.clone()]
    }
    fn dxf_rows(&self) -> Vec<String> {
        vec!["0.0,0.0,0.0".into(), "10.0,20.0,0.0".into()]
    }
    fn clone_box(&self) -> Box<dyn CustomEntity> {
        Box::new(self.clone())
    }
}

const DEFAULT: &[u8] = include_bytes!("../../acad-cmd/tests/fixtures/dim/DPRIOR.dxf");

#[test]
fn generic_and_custom_entities_serialize_to_dxf() {
    let mut drawing = acad_dxf::parse(DEFAULT).unwrap();
    drawing.items.clear();

    drawing.items.push(Item::Entity(Entity::Generic(GenericEntity {
        type_name: "3DFACE".into(),
        layer: 3,
        rows: vec![
            "0.0,0.0,0.0".into(),
            "1.0,0.0,0.0".into(),
            "1.0,1.0,0.0".into(),
            "0.0,1.0,0.0".into(),
        ],
    })));
    drawing.items.push(Item::Entity(Entity::Extension(Box::new(CustomPolyline {
        points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 20.0 }],
        layer: 4,
    }))));

    let output = write(&drawing);
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains("3DFACE,3\r\n0.0,0.0,0.0\r\n1.0,0.0,0.0\r\n1.0,1.0,0.0\r\n0.0,1.0,0.0\r\n"));
    assert!(text.contains("POLYLINE,4\r\n0.0,0.0,0.0\r\n10.0,20.0,0.0\r\n"));
}
