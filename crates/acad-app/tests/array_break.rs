//! Circular ARRAY and BREAK through the Session/API route and file round trips
//! (docs/native-array-break.md).
use acad_app::{api, Session};
use acad_model::{Drawing, Entity};
use serde_json::{json, Value};

fn call(s: &mut Session, method: &str, params: Value) -> Result<Value, String> {
    api::dispatch(
        s,
        serde_json::from_value(json!({"method":method,"params":params})).unwrap(),
        (640, 440),
    )
}
fn commands(s: &mut Session, inputs: &[&str]) {
    for input in inputs {
        call(s, "command", json!({ "input": input }))
            .unwrap_or_else(|error| panic!("{input}: {error}"));
    }
}
fn prompt(s: &mut Session) -> String {
    call(s, "state", json!({})).unwrap()["prompt"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn live(drawing: &Drawing) -> Vec<Entity> {
    drawing.entities().cloned().collect()
}
fn assert_round_trips(drawing: &Drawing) {
    let dwg = acad_dwg::parse(&acad_dwg::write(drawing).unwrap()).unwrap();
    assert_eq!(dwg.items, drawing.items, "DWG keeps every record");
    let dxf = acad_dxf::parse(&acad_dxf::try_write(drawing).unwrap()).unwrap();
    assert_eq!(live(&dxf), live(drawing), "DXF keeps live geometry");
}

#[test]
fn api_break_points_to_the_object_and_undoes_in_one_step() {
    let mut s = Session::default();
    commands(&mut s, &["LAYER", "4", "LINE", "0,0", "10,0", ""]);
    let before = s.drawing().clone();
    commands(&mut s, &["BREAK"]);
    // A miss keeps the prompt and the drawing.
    call(&mut s, "point", json!({"x":5,"y":5})).unwrap();
    assert_eq!(
        prompt(&mut s),
        "BREAK: point to object, or one entity number"
    );
    call(&mut s, "point", json!({"x":2,"y":0})).unwrap();
    assert_eq!(prompt(&mut s), "BREAK: second point or F (first point)");
    // Cancel discards the pick without an undo entry; retry from the start.
    call(&mut s, "cancel", json!({})).unwrap();
    assert_eq!(s.drawing(), &before);
    commands(&mut s, &["BREAK"]);
    call(&mut s, "point", json!({"x":2,"y":0})).unwrap();
    call(&mut s, "point", json!({"x":7,"y":0})).unwrap();
    assert_eq!(prompt(&mut s), "Command");
    assert_eq!(s.drawing().entities().count(), 2);
    assert!(s
        .drawing()
        .entities()
        .all(|e| matches!(e, Entity::OnLayer { layer: 4, .. })));
    assert_round_trips(s.drawing());
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &before);
}

#[test]
fn api_trace_break_with_first_point_reentry_round_trips() {
    let mut s = Session::default();
    commands(&mut s, &["TRACE", "0.5", "1,1", "7,1", "7,6", ""]);
    let before = s.drawing().clone();
    commands(&mut s, &["BREAK", "3,1.25", "F", "1,1", "3,1.25"]);
    assert!(matches!(s.drawing().items[0], acad_model::Item::Erased(_)));
    assert_eq!(s.drawing().entities().count(), 2);
    assert_round_trips(s.drawing());
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &before);
}

#[test]
fn api_rotated_block_array_round_trips_and_undoes_in_one_step() {
    let mut s = Session::default();
    commands(
        &mut s,
        &[
            "LINE", "1,0", "2,0", "", "BLOCK", "B", "0,0", "LAST", "INSERT", "B", "5,3", "1", "1",
            "10",
        ],
    );
    let before = s.drawing().clone();
    commands(&mut s, &["ARRAY", "L", "C", "4,3", "90", "-270"]);
    assert_eq!(prompt(&mut s), "ARRAY: rotate block as it is copied? <N>");
    commands(&mut s, &["Y"]);
    let rotations: Vec<_> = s
        .drawing()
        .entities()
        .filter_map(|e| match e {
            Entity::OnLayer { entity, .. } => match entity.as_ref() {
                Entity::Insert { rotation_deg, .. } => Some(*rotation_deg),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(rotations.len(), 4);
    for (got, expected) in rotations.iter().zip([10.0, 100.0, 190.0, 280.0]) {
        assert!((got - expected).abs() < 1e-9, "{rotations:?}");
    }
    assert_round_trips(s.drawing());
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &before);
}

#[test]
fn api_collected_selection_reaches_break_as_ids() {
    let mut s = Session::default();
    commands(
        &mut s,
        &["LINE", "0,2", "10,2", "", "LINE", "0,5", "10,5", ""],
    );
    let before = s.drawing().clone();
    // Window collects LINE 1, an API pick collects LINE 2; Return must not
    // read the transport text "1,2" as a pick point on LINE 1.
    commands(&mut s, &["BREAK", "W", "-1,1", "11,3"]);
    call(&mut s, "point", json!({"x":5,"y":5})).unwrap();
    assert_eq!(s.input(), "1,2");
    assert!(call(&mut s, "command", json!({"input":""})).is_err());
    assert_eq!(s.drawing(), &before);
    call(&mut s, "cancel", json!({})).unwrap();
    // One collected object continues with explicit points.
    commands(&mut s, &["BREAK", "W", "-1,4", "11,6", ""]);
    assert_eq!(prompt(&mut s), "BREAK: first point");
    commands(&mut s, &["2,5", "7,5"]);
    assert_eq!(s.drawing().entities().count(), 3);
    commands(&mut s, &["UNDO"]);
    assert_eq!(s.drawing(), &before);
}
