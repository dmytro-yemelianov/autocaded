//! Compact visible bounds for windows and the INSERT-origin pick gate.
use super::*;

const MAX_STORED_VISITS: usize = 100_000;
const MAX_DEPTH: usize = 256;

// REPEAT markers have no own layer. Layer wrappers gate a whole group;
// LOAD is metadata and never provides selectable geometry.
pub(super) fn visible_record(drawing: &Drawing, mut entity: &Entity) -> bool {
    let mut wrapped = false;
    while let Entity::OnLayer {
        layer,
        entity: child,
    } = entity
    {
        if !drawing.header.layer_is_visible(*layer) {
            return false;
        }
        wrapped = true;
        entity = child;
    }
    !matches!(entity, Entity::Load { .. } | Entity::Erased(_))
        && (wrapped || matches!(entity, Entity::Repeat(_)) || drawing.header.layer_is_visible(1))
}

pub(super) fn item_bounds(drawing: &Drawing, item: &Item) -> Option<[Point; 2]> {
    let mut remaining = MAX_STORED_VISITS;
    match item {
        Item::Entity(entity) => entity_bounds(drawing, entity, &mut remaining, 0, 0),
        Item::Repeat(repeat) => repeat_bounds(drawing, repeat, &mut remaining, 0, 0),
        _ => Ok(None),
    }
    .ok()
    .flatten()
}

fn merge(bounds: &mut Option<[Point; 2]>, child: [Point; 2]) {
    *bounds = Some(match *bounds {
        None => child,
        Some([min, max]) => [
            Point {
                x: min.x.min(child[0].x),
                y: min.y.min(child[0].y),
            },
            Point {
                x: max.x.max(child[1].x),
                y: max.y.max(child[1].y),
            },
        ],
    });
}
fn points_bounds(points: impl IntoIterator<Item = Point>) -> Result<Option<[Point; 2]>, ()> {
    let mut out = None;
    for point in points {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(());
        }
        merge(&mut out, [point, point]);
    }
    Ok(out)
}

pub(super) fn entity_bounds(
    drawing: &Drawing,
    entity: &Entity,
    remaining: &mut usize,
    depth: usize,
    inserts: usize,
) -> Result<Option<[Point; 2]>, ()> {
    if depth > MAX_DEPTH || *remaining == 0 {
        return Err(());
    }
    *remaining -= 1;
    let mut leaf = entity;
    let mut depth = depth;
    while let Entity::OnLayer { entity: child, .. } = leaf {
        if depth >= MAX_DEPTH || *remaining == 0 {
            return Err(());
        }
        depth += 1;
        *remaining -= 1;
        leaf = child;
    }
    if !visible_record(drawing, entity) {
        return Ok(None);
    }
    match bare(entity) {
        Entity::Repeat(repeat) => repeat_bounds(drawing, repeat, remaining, depth + 1, inserts),
        Entity::Insert {
            name,
            origin,
            x_scale,
            y_scale,
            rotation_deg,
        } => {
            if inserts >= 16 {
                return Err(());
            }
            let Some(block) = drawing.block(name) else {
                return Ok(None);
            };
            let mut bounds = None;
            for child in &block.entities {
                if let Some(child) =
                    entity_bounds(drawing, child, remaining, depth + 1, inserts + 1)?
                {
                    merge(&mut bounds, child);
                }
            }
            let Some([min, max]) = bounds else {
                return Ok(None);
            };
            let (sin, cos) = rotation_deg.to_radians().sin_cos();
            points_bounds(
                [
                    min,
                    Point { x: min.x, y: max.y },
                    max,
                    Point { x: max.x, y: min.y },
                ]
                .map(|point| {
                    let x = (point.x - block.base.x) * x_scale;
                    let y = (point.y - block.base.y) * y_scale;
                    Point {
                        x: origin.x + x * cos - y * sin,
                        y: origin.y + x * sin + y * cos,
                    }
                }),
            )
        }
        leaf => points_bounds(selection_extents_points(leaf).unwrap_or_default()),
    }
}

fn repeat_bounds(
    drawing: &Drawing,
    repeat: &acad_model::Repeat,
    remaining: &mut usize,
    depth: usize,
    inserts: usize,
) -> Result<Option<[Point; 2]>, ()> {
    if depth > MAX_DEPTH {
        return Err(());
    }
    if repeat.rows == 0 || repeat.columns == 0 {
        return Ok(None);
    }
    let mut bounds = None;
    for child in &repeat.entities {
        if let Some(child) = entity_bounds(drawing, child, remaining, depth + 1, inserts)? {
            merge(&mut bounds, child);
        }
    }
    let Some([min, max]) = bounds else {
        return Ok(None);
    };
    let dx = f64::from(repeat.columns - 1) * repeat.column_spacing;
    let dy = f64::from(repeat.rows - 1) * repeat.row_spacing;
    points_bounds([
        Point {
            x: min.x + dx.min(0.0),
            y: min.y + dy.min(0.0),
        },
        Point {
            x: max.x + dx.max(0.0),
            y: max.y + dy.max(0.0),
        },
    ])
}

/// Compact visible drawing bounds; one shared budget covers every live owner.
/// Navigation fails instead of silently omitting oversized/invalid geometry.
pub(crate) fn drawing_bounds(drawing: &Drawing) -> Result<Option<acad_model::Extents>, String> {
    let mut remaining = MAX_STORED_VISITS;
    let mut bounds = None;
    for item in &drawing.items {
        let child = match item {
            Item::Entity(entity) => entity_bounds(drawing, entity, &mut remaining, 0, 0),
            Item::Repeat(repeat) => repeat_bounds(drawing, repeat, &mut remaining, 0, 0),
            Item::Block(_) | Item::Erased(_) => continue,
        }
        .map_err(|()| "visible drawing bounds exceed finite geometry/traversal limits")?;
        if let Some(child) = child {
            merge(&mut bounds, child);
        }
    }
    Ok(bounds.map(|[min, max]| acad_model::Extents {
        xmin: min.x,
        ymin: min.y,
        xmax: max.x,
        ymax: max.y,
    }))
}
