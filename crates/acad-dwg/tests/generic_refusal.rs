use acad_dwg::{header::Version, write_version};
use acad_model::{CustomEntity, Entity, Extents, GenericEntity, Item};

#[derive(Debug, Clone)]
struct CustomPolyline;

impl CustomEntity for CustomPolyline {
    fn type_name(&self) -> &str {
        "POLYLINE"
    }
    fn bounding_extents(&self) -> Option<Extents> {
        None
    }
    fn clone_box(&self) -> Box<dyn CustomEntity> {
        Box::new(self.clone())
    }
}

const DEFAULT: &[u8] = include_bytes!("../../acad-cmd/tests/fixtures/dim/DPRIOR.dwg");

#[test]
fn writing_generic_entity_to_1983_dwg_is_refused() {
    let mut drawing = acad_dwg::parse(DEFAULT).unwrap();
    drawing.items.clear();
    drawing
        .items
        .push(Item::Entity(Entity::Generic(GenericEntity {
            type_name: "3DFACE".into(),
            layer: 1,
            rows: vec!["0,0,0".into()],
        })));

    let err = write_version(&drawing, Version::Ac140).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("3DFACE"));
    assert!(msg.contains("not supported in 1983 AutoCAD DWG format"));
}

#[test]
fn writing_custom_entity_to_1983_dwg_is_refused() {
    let mut drawing = acad_dwg::parse(DEFAULT).unwrap();
    drawing.header.dim_arrow = None;
    drawing.items.clear();
    drawing
        .items
        .push(Item::Entity(Entity::Extension(Box::new(CustomPolyline))));

    let err = write_version(&drawing, Version::Ac12).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("POLYLINE"));
    assert!(msg.contains("not supported in 1983 AutoCAD DWG format"));
}
