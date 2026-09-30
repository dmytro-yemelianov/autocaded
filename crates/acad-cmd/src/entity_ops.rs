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
