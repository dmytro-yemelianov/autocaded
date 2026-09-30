use acad_model::{Drawing, Entity, Item, Point};
use std::collections::BTreeSet;

use crate::bare;

pub(crate) fn entities_in_window(drawing: &Drawing, first: Point, second: Point) -> Vec<usize> {
    let min = Point {
        x: first.x.min(second.x),
        y: first.y.min(second.y),
    };
    let max = Point {
        x: first.x.max(second.x),
        y: first.y.max(second.y),
    };
    let mut ids = Vec::new();
    let mut selectable_id = 0;
    for item in &drawing.items {
        let Item::Entity(entity) = item else {
            continue;
        };
        if matches!(bare(entity), Entity::Load { .. }) {
            continue;
        }
        selectable_id += 1;
        let Some(points) = selection_extents_points(entity) else {
            continue;
        };
        if points.iter().all(|point| {
            point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
        }) {
            ids.push(selectable_id);
        }
    }
    ids
}

pub(crate) fn selection_extents_points(entity: &Entity) -> Option<Vec<Point>> {
    Some(match bare(entity) {
        Entity::Line { start, end } => vec![*start, *end],
        Entity::Circle { center, radius } | Entity::Arc { center, radius, .. } => vec![
            Point {
                x: center.x - radius,
                y: center.y - radius,
            },
            Point {
                x: center.x + radius,
                y: center.y + radius,
            },
        ],
        Entity::Point { origin } | Entity::Text { origin, .. } | Entity::Shape { origin, .. } => {
            vec![*origin]
        }
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            vec![*p1, *p2, *p3, *p4]
        }
        Entity::Repeat(repeat) => repeat
            .entities
            .iter()
            .filter_map(selection_extents_points)
            .flatten()
            .collect(),
        Entity::Insert { .. } | Entity::Load { .. } | Entity::OnLayer { .. } => return None,
    })
}

pub(crate) fn selectable_count(drawing: &Drawing) -> usize {
    drawing
        .items
        .iter()
        .filter(|item| {
            matches!(item, Item::Entity(entity) if !matches!(bare(entity), Entity::Load { .. }))
        })
        .count()
}

pub(crate) fn selection(input: &str, count: usize) -> Result<Vec<usize>, String> {
    if input.eq_ignore_ascii_case("ALL") {
        if count == 0 {
            return Err("there are no selectable entities".into());
        }
        return Ok((1..=count).collect());
    }
    if input.eq_ignore_ascii_case("LAST") || input.eq_ignore_ascii_case("L") {
        return if count == 0 {
            Err("there are no selectable entities".into())
        } else {
            Ok(vec![count])
        };
    }
    let mut ids = BTreeSet::new();
    for part in input.split(',') {
        let id = part
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("invalid entity number: {}", part.trim()))?;
        if id == 0 || id > count {
            return Err(format!("entity number {id} is out of range (1..={count})"));
        }
        ids.insert(id);
    }
    if ids.is_empty() {
        return Err("select at least one entity".into());
    }
    Ok(ids.into_iter().collect())
}

pub(crate) fn selected_item_indexes(drawing: &Drawing, ids: &[usize]) -> BTreeSet<usize> {
    let wanted: BTreeSet<_> = ids.iter().copied().collect();
    let mut selectable = 0usize;
    drawing
        .items
        .iter()
        .enumerate()
        .filter_map(|(item_index, item)| match item {
            Item::Entity(entity) if !matches!(bare(entity), Entity::Load { .. }) => {
                selectable += 1;
                wanted.contains(&selectable).then_some(item_index + 1)
            }
            _ => None,
        })
        .collect()
}

pub(crate) fn entity_pick_distance(point: Point, entity: &Entity) -> Option<f64> {
    let distance = |a: Point, b: Point| (a.x - b.x).hypot(a.y - b.y);
    let segment_distance = |start: Point, end: Point| {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_squared = dx * dx + dy * dy;
        if length_squared == 0.0 {
            return distance(point, start);
        }
        let along = (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared)
            .clamp(0.0, 1.0);
        distance(
            point,
            Point {
                x: start.x + along * dx,
                y: start.y + along * dy,
            },
        )
    };
    match bare(entity) {
        Entity::Line { start, end } => Some(segment_distance(*start, *end)),
        Entity::Circle { center, radius } => Some((distance(point, *center) - radius).abs()),
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            let radial = distance(point, *center);
            let angle = (point.y - center.y)
                .atan2(point.x - center.x)
                .to_degrees()
                .rem_euclid(360.0);
            let sweep = end_deg - start_deg;
            let offset = (angle - start_deg).rem_euclid(360.0);
            if sweep.abs() >= 360.0 || offset <= sweep.rem_euclid(360.0) {
                Some((radial - radius).abs())
            } else {
                let endpoint = |degrees: f64| Point {
                    x: center.x + radius * degrees.to_radians().cos(),
                    y: center.y + radius * degrees.to_radians().sin(),
                };
                Some(distance(point, endpoint(*start_deg)).min(distance(point, endpoint(*end_deg))))
            }
        }
        Entity::Point { origin }
        | Entity::Text { origin, .. }
        | Entity::Shape { origin, .. }
        | Entity::Insert { origin, .. } => Some(distance(point, *origin)),
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => Some(
            [(*p1, *p2), (*p2, *p3), (*p3, *p4), (*p4, *p1)]
                .into_iter()
                .map(|(start, end)| segment_distance(start, end))
                .fold(f64::INFINITY, f64::min),
        ),
        Entity::Repeat(_) | Entity::Load { .. } | Entity::OnLayer { .. } => None,
    }
}
