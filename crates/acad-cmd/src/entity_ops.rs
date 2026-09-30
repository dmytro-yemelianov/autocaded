use crate::input_state::Transform;
use acad_model::{Entity, Item, Point};
use std::collections::BTreeSet;

pub(crate) fn collect_insert_names(entity: &Entity, names: &mut BTreeSet<String>) {
    match entity {
        Entity::OnLayer { entity, .. } => collect_insert_names(entity, names),
        Entity::Insert { name, .. } => {
            names.insert(name.to_ascii_uppercase());
        }
        Entity::Repeat(repeat) => {
            for entity in &repeat.entities {
                collect_insert_names(entity, names);
            }
        }
        _ => {}
    }
}

pub(crate) fn transform_entity(entity: &mut Entity, transform: Transform) {
    if let Entity::OnLayer { entity, .. } = entity {
        transform_entity(entity, transform);
        return;
    }
    let point = |p: &mut Point| match transform {
        Transform::Translate(delta) => {
            p.x += delta.x;
            p.y += delta.y;
        }
        Transform::Rotate { base, degrees } => {
            let angle = degrees.to_radians();
            let (sin, cos) = angle.sin_cos();
            let x = p.x - base.x;
            let y = p.y - base.y;
            p.x = base.x + x * cos - y * sin;
            p.y = base.y + x * sin + y * cos;
        }
        Transform::Scale { base, factor } => {
            p.x = base.x + (p.x - base.x) * factor;
            p.y = base.y + (p.y - base.y) * factor;
        }
    };
    match entity {
        Entity::Repeat(repeat) => {
            for inner in &mut repeat.entities {
                transform_entity(inner, transform);
            }
            if let Transform::Scale { factor, .. } = transform {
                repeat.column_spacing *= factor;
                repeat.row_spacing *= factor;
            }
        }
        Entity::Line { start, end } => {
            point(start);
            point(end);
        }
        Entity::Circle { center, radius } => {
            point(center);
            if let Transform::Scale { factor, .. } = transform {
                *radius *= factor;
            }
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            point(center);
            match transform {
                Transform::Scale { factor, .. } => *radius *= factor,
                Transform::Rotate { degrees, .. } => {
                    *start_deg = (*start_deg + degrees).rem_euclid(360.0);
                    *end_deg = (*end_deg + degrees).rem_euclid(360.0);
                }
                Transform::Translate(_) => {}
            }
        }
        Entity::Text {
            origin,
            height,
            rotation_deg,
            ..
        }
        | Entity::Shape {
            origin,
            height,
            rotation_deg,
            ..
        } => {
            point(origin);
            match transform {
                Transform::Scale { factor, .. } => *height *= factor,
                Transform::Rotate { degrees, .. } => *rotation_deg += degrees,
                Transform::Translate(_) => {}
            }
        }
        Entity::Insert {
            origin,
            x_scale,
            y_scale,
            rotation_deg,
            ..
        } => {
            point(origin);
            match transform {
                Transform::Scale { factor, .. } => {
                    *x_scale *= factor;
                    *y_scale *= factor;
                }
                Transform::Rotate { degrees, .. } => *rotation_deg += degrees,
                Transform::Translate(_) => {}
            }
        }
        Entity::Point { origin } => point(origin),
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            point(p1);
            point(p2);
            point(p3);
            point(p4);
        }
        Entity::Load { .. } => {}
        Entity::OnLayer { .. } => unreachable!("layer wrapper was removed above"),
    }
}

pub(crate) fn bare(mut entity: &Entity) -> &Entity {
    while let Entity::OnLayer { entity: inner, .. } = entity {
        entity = inner;
    }
    entity
}

pub(crate) fn normalize_library_name(name: &str) -> String {
    let leaf = name
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_uppercase();
    leaf.strip_suffix(".SHP").unwrap_or(&leaf).to_owned()
}

pub(crate) fn load_library_name(entity: &Entity) -> Option<String> {
    match bare(entity) {
        Entity::Load { name } => Some(normalize_library_name(name)),
        _ => None,
    }
}

pub(crate) fn entity_anchor(entity: &Entity) -> Option<Point> {
    match bare(entity) {
        Entity::Repeat(repeat) => repeat.entities.first().and_then(entity_anchor),
        Entity::Line { start, .. } => Some(*start),
        Entity::Circle { center, .. } | Entity::Arc { center, .. } => Some(*center),
        Entity::Point { origin } | Entity::Text { origin, .. } | Entity::Shape { origin, .. } => {
            Some(*origin)
        }
        Entity::Trace { p1, .. } | Entity::Solid { p1, .. } => Some(*p1),
        Entity::Insert { origin, .. } => Some(*origin),
        Entity::Load { .. } | Entity::OnLayer { .. } => None,
    }
}

pub(crate) fn item_anchor(item: &Item) -> Option<Point> {
    match item {
        Item::Entity(entity) => entity_anchor(entity),
        _ => None,
    }
}

pub(crate) fn can_change_point(entity: &Entity) -> bool {
    matches!(
        bare(entity),
        Entity::Line { .. } | Entity::Circle { .. } | Entity::Insert { .. }
    )
}

pub(crate) fn apply_change_point(entity: &mut Entity, point: Point) {
    match entity {
        Entity::OnLayer { entity, .. } => apply_change_point(entity, point),
        Entity::Line { start, end } => {
            let distance = |candidate: Point| {
                (candidate.x - point.x).powi(2) + (candidate.y - point.y).powi(2)
            };
            if distance(*start) <= distance(*end) {
                *start = point;
            } else {
                *end = point;
            }
        }
        Entity::Circle { center, radius } => {
            *radius = ((center.x - point.x).powi(2) + (center.y - point.y).powi(2)).sqrt();
        }
        Entity::Insert { origin, .. } => *origin = point,
        Entity::Repeat(_)
        | Entity::Load { .. }
        | Entity::Shape { .. }
        | Entity::Arc { .. }
        | Entity::Text { .. }
        | Entity::Point { .. }
        | Entity::Trace { .. }
        | Entity::Solid { .. } => unreachable!("unsupported CHANGE point entity was validated"),
    }
}

pub(crate) fn set_insert_angle(entity: &mut Entity, angle: f64) {
    match entity {
        Entity::OnLayer { entity, .. } => set_insert_angle(entity, angle),
        Entity::Insert { rotation_deg, .. } => *rotation_deg = angle,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;

    fn bare_first(editor: &Editor) -> &Entity {
        bare(editor.drawing.entities().next().unwrap())
    }

    fn layer_of(entity: &Entity) -> u8 {
        match entity {
            Entity::OnLayer { layer, .. } => *layer,
            _ => 1,
        }
    }

    #[test]
    fn insert_places_a_case_insensitive_block_with_default_or_explicit_scaling() {
        let mut editor = Editor::default();
        editor
            .drawing_mut()
            .items
            .push(Item::Block(acad_model::Block {
                name: "SYMBOL".into(),
                base: Point { x: 1.0, y: 2.0 },
                entities: vec![Entity::Line {
                    start: Point { x: 1.0, y: 2.0 },
                    end: Point { x: 3.0, y: 2.0 },
                }],
            }));
        for input in ["INSERT", "symbol", "10,20", "", "", ""] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            bare_first(&editor),
            &Entity::Insert {
                origin: Point { x: 10.0, y: 20.0 },
                x_scale: 1.0,
                y_scale: 1.0,
                rotation_deg: 0.0,
                name: "SYMBOL".into(),
            }
        );
        for input in ["INSERT", "SYMBOL", "-2,4", "2", "3", "45"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            bare(editor.drawing().entities().nth(1).unwrap()),
            &Entity::Insert {
                origin: Point { x: -2.0, y: 4.0 },
                x_scale: 2.0,
                y_scale: 3.0,
                rotation_deg: 45.0,
                name: "SYMBOL".into(),
            }
        );
        assert!(editor.submit("UNDO").is_ok());
        assert_eq!(editor.drawing().entities().count(), 1);
        for input in ["INSERT", "SYMBOL", "0,0"] {
            editor.submit(input).unwrap();
        }
        assert!(editor.submit("0").is_err());
        assert_eq!(editor.drawing().entities().count(), 1);
        assert_eq!(
            editor.prompt(),
            "INSERT: X scale or opposite corner x,y (Enter for 1)"
        );
    }

    #[test]
    fn star_insert_explodes_a_block_after_translating_from_its_base() {
        let mut editor = Editor::default();
        for input in ["LINE", "2,3", "4,5", "", "BLOCK", "B1", "1,2", "LAST"] {
            editor.submit(input).unwrap();
        }
        let before = editor.drawing().clone();
        for input in ["INSERT", "*b1", "7,8"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing().blocks().count(), 1);
        assert_eq!(
            bare(editor.drawing().entities().next().unwrap()),
            &Entity::Line {
                start: Point { x: 8.0, y: 9.0 },
                end: Point { x: 10.0, y: 11.0 },
            }
        );
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &before);
    }

    #[test]
    fn insert_opposite_corner_sets_both_scales_and_skips_the_y_prompt() {
        let mut editor = Editor::default();
        for input in ["LINE", "2,3", "4,5", "", "BLOCK", "B1", "1,2", "LAST"] {
            editor.submit(input).unwrap();
        }
        for input in ["INSERT", "B1", "3,3", "5,6"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.prompt(), "INSERT: rotation angle (Enter for 0)");
        editor.submit("30").unwrap();
        assert_eq!(
            bare(editor.drawing().entities().next().unwrap()),
            &Entity::Insert {
                origin: Point { x: 3.0, y: 3.0 },
                x_scale: 2.0,
                y_scale: 3.0,
                rotation_deg: 30.0,
                name: "B1".into(),
            }
        );
    }

    #[test]
    fn move_copy_rotate_scale_and_undo_transform_model_geometry() {
        let mut editor = Editor::default();
        for input in ["LINE", "1,0", "2,0", ""] {
            editor.submit(input).unwrap();
        }
        for input in ["MOVE", "0,0", "1,2", "1"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            bare_first(&editor),
            &Entity::Line {
                start: Point { x: 2.0, y: 2.0 },
                end: Point { x: 3.0, y: 2.0 },
            }
        );
        for input in ["COPY", "0,0", "-1,0", "1"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.drawing().entities().count(), 2);
        for input in ["ROTATE", "1", "0,0", "90"] {
            editor.submit(input).unwrap();
        }
        let Entity::Line { start, end } = bare_first(&editor) else {
            panic!("expected the selected line");
        };
        assert!((start.x + 2.0).abs() < 1e-12);
        assert!((start.y - 2.0).abs() < 1e-12);
        assert!((end.x + 2.0).abs() < 1e-12);
        assert!((end.y - 3.0).abs() < 1e-12);
        for input in ["SCALE", "2", "0,0", "2"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(
            bare(editor.drawing().entities().nth(1).unwrap()),
            &Entity::Line {
                start: Point { x: 2.0, y: 4.0 },
                end: Point { x: 4.0, y: 4.0 },
            }
        );
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().entities().count(), 2);
        assert_eq!(
            bare(editor.drawing().entities().nth(1).unwrap()),
            &Entity::Line {
                start: Point { x: 1.0, y: 2.0 },
                end: Point { x: 2.0, y: 2.0 },
            }
        );
    }

    #[test]
    fn rectangular_array_copies_selected_geometry_by_signed_row_and_column_spacing() {
        let mut editor = Editor::default();
        for input in ["LINE", "0,0", "1,0", ""] {
            editor.submit(input).unwrap();
        }
        for input in ["ARRAY", "1", "R", "2", "3", "-5", "10"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.drawing().entities().count(), 6);
        let lines: Vec<_> = editor
            .drawing()
            .entities()
            .map(|entity| match bare(entity) {
                Entity::Line { start, end } => (*start, *end),
                other => panic!("expected array line, got {other:?}"),
            })
            .collect();
        assert_eq!(
            lines,
            [
                (Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 0.0 }),
                (Point { x: 0.0, y: -5.0 }, Point { x: 1.0, y: -5.0 }),
                (Point { x: 10.0, y: 0.0 }, Point { x: 11.0, y: 0.0 }),
                (Point { x: 10.0, y: -5.0 }, Point { x: 11.0, y: -5.0 }),
                (Point { x: 20.0, y: 0.0 }, Point { x: 21.0, y: 0.0 }),
                (Point { x: 20.0, y: -5.0 }, Point { x: 21.0, y: -5.0 }),
            ]
        );
        assert_eq!(editor.status(), "Created 2x3 array with 5 copies");
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().entities().count(), 1);
    }

    #[test]
    fn change_moves_selected_entities_to_a_layer_and_undo_restores_header_and_geometry() {
        let mut editor = Editor::default();
        for input in ["POINT", "1,2", "LAYER", "2", "POINT", "3,4"] {
            editor.submit(input).unwrap();
        }
        let before: Vec<_> = editor.drawing().entities().cloned().collect();
        for input in ["CHANGE", "1", "L", "7"] {
            editor.submit(input).unwrap();
        }
        let changed: Vec<_> = editor.drawing().entities().cloned().collect();
        assert_eq!(bare(&changed[0]), bare(&before[0]));
        assert_eq!(layer_of(&changed[0]), 7);
        assert_eq!(changed[1], before[1]);
        assert_eq!(editor.drawing().header.layers.get(&7), Some(&15));
        assert_eq!(editor.status(), "Changed 1 entities to layer 7");

        editor.submit("UNDO").unwrap();
        assert_eq!(
            editor.drawing().entities().cloned().collect::<Vec<_>>(),
            before
        );
        assert!(!editor.drawing().header.layers.contains_key(&7));
    }

    #[test]
    fn fillet_trims_two_intersecting_lines_and_adds_the_tangent_arc() {
        let mut editor = Editor::default();
        for input in [
            "LINE", "-5,0", "5,0", "", "LINE", "0,-5", "0,5", "", "FILLET", "1,2", "1",
        ] {
            editor.submit(input).unwrap();
        }
        let mut entities = editor.drawing().entities();
        let Entity::OnLayer { entity, .. } = entities.next().unwrap() else {
            panic!("expected layered line");
        };
        assert!(
            matches!(entity.as_ref(), Entity::Line { start: Point { x, y: 0.0 }, end: Point { x: 5.0, y: 0.0 } } if (*x - 1.0).abs() < 1e-10)
        );
        let Entity::OnLayer { entity, .. } = entities.next().unwrap() else {
            panic!("expected layered line");
        };
        assert!(
            matches!(entity.as_ref(), Entity::Line { start: Point { x: 0.0, y: -5.0 }, end: Point { x: 0.0, y, } } if (*y + 1.0).abs() < 1e-10)
        );
        let Entity::OnLayer { entity, .. } = entities.next().unwrap() else {
            panic!("expected layered arc");
        };
        let Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } = entity.as_ref()
        else {
            panic!("expected fillet arc");
        };
        assert!((center.x - 1.0).abs() < 1e-10);
        assert!((center.y + 1.0).abs() < 1e-10);
        assert_eq!(*radius, 1.0);
        assert!((start_deg - 90.0).abs() < 1e-10, "start={start_deg}");
        assert!((end_deg - 180.0).abs() < 1e-10, "end={end_deg}");
        assert_eq!(editor.status(), "Filleted two lines");

        drop(entities);
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().entities().count(), 2);
        assert_eq!(
            bare(editor.drawing().entities().next().unwrap()),
            &Entity::Line {
                start: Point { x: -5.0, y: 0.0 },
                end: Point { x: 5.0, y: 0.0 },
            }
        );
    }

    #[test]
    fn break_removes_the_segment_between_two_points_and_preserves_layer() {
        let mut editor = Editor::default();
        for input in [
            "LAYER", "7", "LINE", "0,0", "10,0", "", "BREAK", "1", "7,0", "2,0",
        ] {
            editor.submit(input).unwrap();
        }
        let pieces: Vec<_> = editor.drawing().entities().cloned().collect();
        assert_eq!(pieces.len(), 2);
        assert_eq!(layer_of(&pieces[0]), 7);
        assert_eq!(layer_of(&pieces[1]), 7);
        assert_eq!(
            bare(&pieces[0]),
            &Entity::Line {
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 2.0, y: 0.0 },
            }
        );
        assert_eq!(
            bare(&pieces[1]),
            &Entity::Line {
                start: Point { x: 7.0, y: 0.0 },
                end: Point { x: 10.0, y: 0.0 },
            }
        );
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing().entities().count(), 1);
        assert_eq!(layer_of(editor.drawing().entities().next().unwrap()), 7);
    }

    #[test]
    fn break_circle_retains_the_complementary_arc_across_zero_degrees() {
        let mut editor = Editor::default();
        for input in ["CIRCLE", "0,0", "5"] {
            editor.submit(input).unwrap();
        }
        let point_at = |degrees: f64| {
            let radians = degrees.to_radians();
            format!("{:.12},{:.12}", 5.0 * radians.cos(), 5.0 * radians.sin())
        };
        for input in ["BREAK", "1"] {
            editor.submit(input).unwrap();
        }
        editor.submit(&point_at(350.0)).unwrap();
        editor.submit(&point_at(10.0)).unwrap();
        let Entity::OnLayer { entity, .. } = editor.drawing().entities().next().unwrap() else {
            panic!("expected layered arc");
        };
        let Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } = entity.as_ref()
        else {
            panic!("BREAK should turn the retained circle into one arc");
        };
        assert_eq!(*center, Point { x: 0.0, y: 0.0 });
        assert_eq!(*radius, 5.0);
        assert!((*start_deg - 10.0).abs() < 1e-8);
        assert!((*end_deg - 350.0).abs() < 1e-8);
        editor.submit("UNDO").unwrap();
        assert!(matches!(
            bare(editor.drawing().entities().next().unwrap()),
            Entity::Circle { radius: 5.0, .. }
        ));
    }

    #[test]
    fn break_arc_creates_two_arcs_when_the_source_sweep_crosses_zero() {
        let mut editor = Editor::default();
        editor
            .drawing_mut()
            .items
            .push(Item::Entity(Entity::OnLayer {
                layer: 4,
                entity: Box::new(Entity::Arc {
                    center: Point { x: 0.0, y: 0.0 },
                    radius: 5.0,
                    start_deg: 300.0,
                    end_deg: 60.0,
                }),
            }));
        let point_at = |degrees: f64| {
            let radians = degrees.to_radians();
            format!("{:.12},{:.12}", 5.0 * radians.cos(), 5.0 * radians.sin())
        };
        for input in ["BREAK", "1"] {
            editor.submit(input).unwrap();
        }
        editor.submit(&point_at(330.0)).unwrap();
        editor.submit(&point_at(30.0)).unwrap();

        let arcs: Vec<_> = editor
            .drawing()
            .entities()
            .map(|entity| {
                assert_eq!(layer_of(entity), 4);
                let Entity::Arc {
                    start_deg, end_deg, ..
                } = bare(entity)
                else {
                    panic!("BREAK should preserve both sides as arcs");
                };
                (*start_deg, *end_deg)
            })
            .collect();
        assert_eq!(arcs.len(), 2);
        assert!((arcs[0].0 - 300.0).abs() < 1e-8);
        assert!((arcs[0].1 - 330.0).abs() < 1e-8);
        assert!((arcs[1].0 - 30.0).abs() < 1e-8);
        assert!((arcs[1].1 - 60.0).abs() < 1e-8);

        editor.submit("UNDO").unwrap();
        let Entity::Arc {
            start_deg, end_deg, ..
        } = bare(editor.drawing().entities().next().unwrap())
        else {
            panic!("undo should restore the source arc");
        };
        assert_eq!((*start_deg, *end_deg), (300.0, 60.0));
    }

    #[test]
    fn block_moves_last_entity_into_a_named_definition_and_undo_restores_it() {
        let mut editor = Editor::default();
        for input in ["LINE", "2,3", "4,5", "", "CIRCLE", "5,6", "1"] {
            editor.submit(input).unwrap();
        }
        let before = editor.drawing().clone();
        for input in ["BLOCK", "b1", "1,2", "LAST"] {
            editor.submit(input).unwrap();
        }
        assert_eq!(editor.prompt(), "Command");
        assert_eq!(editor.drawing().entities().count(), 1);
        let block = editor.drawing().block("B1").unwrap();
        assert_eq!(block.base, Point { x: 1.0, y: 2.0 });
        assert_eq!(block.entities.len(), 1);
        assert!(matches!(bare(&block.entities[0]), Entity::Circle { .. }));
        editor.submit("UNDO").unwrap();
        assert_eq!(editor.drawing(), &before);
    }
}
