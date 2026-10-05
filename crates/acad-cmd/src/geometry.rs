//! Pure geometry helpers: hatch, break, fillet and area/perimeter math.

use crate::entity_ops::bare;
use crate::hatch_pattern::DashPattern;
use crate::MAX_ARRAY_ENTITIES;
use acad_model::{Drawing, Entity, Item, Point};
use std::collections::BTreeSet;

pub(crate) mod arc;
pub(crate) mod circle;
mod hatch;
pub(crate) use hatch::{hatch_geometry, HatchRequest, HatchSpec, HatchStyle};

/// Bounds are aggregated once per stored descendant, never per generated cell.
/// An explicit iterator stack avoids recursive calls and sibling duplication:
/// output is two points; auxiliary storage is bounded by stored nesting depth.
pub(crate) fn repeat_bounds_points(
    repeat: &acad_model::Repeat,
    strict: bool,
) -> Option<[Point; 2]> {
    if repeat.rows == 0 || repeat.columns == 0 {
        return None;
    }
    let offsets = |group: &acad_model::Repeat| {
        let dx = f64::from(group.columns - 1) * group.column_spacing;
        let dy = f64::from(group.rows - 1) * group.row_spacing;
        (dx.min(0.0), dx.max(0.0), dy.min(0.0), dy.max(0.0))
    };
    let mut stack = vec![(repeat.entities.iter(), offsets(repeat))];
    let mut min = Point {
        x: f64::INFINITY,
        y: f64::INFINITY,
    };
    let mut max = Point {
        x: f64::NEG_INFINITY,
        y: f64::NEG_INFINITY,
    };
    let mut found = false;
    while let Some((entities, offset)) = stack.last_mut() {
        let Some(entity) = entities.next() else {
            stack.pop();
            continue;
        };
        let offset = *offset;
        match bare(entity) {
            Entity::Repeat(group) => {
                if group.rows == 0 || group.columns == 0 {
                    continue;
                }
                let child = offsets(group);
                stack.push((
                    group.entities.iter(),
                    (
                        offset.0 + child.0,
                        offset.1 + child.1,
                        offset.2 + child.2,
                        offset.3 + child.3,
                    ),
                ));
            }
            Entity::Load { .. } => {}
            Entity::Insert { .. } if strict => return None,
            entity => {
                let mut points = Vec::with_capacity(4);
                entity_points(entity, &mut points);
                for point in points {
                    let low = Point {
                        x: point.x + offset.0,
                        y: point.y + offset.2,
                    };
                    let high = Point {
                        x: point.x + offset.1,
                        y: point.y + offset.3,
                    };
                    if ![low.x, low.y, high.x, high.y]
                        .iter()
                        .all(|value| value.is_finite())
                    {
                        return None;
                    }
                    min.x = min.x.min(low.x);
                    min.y = min.y.min(low.y);
                    max.x = max.x.max(high.x);
                    max.y = max.y.max(high.y);
                    found = true;
                }
            }
        }
    }
    found.then_some([min, max])
}

pub(crate) fn repeat_points(repeat: &acad_model::Repeat, out: &mut Vec<Point>) {
    if let Some(points) = repeat_bounds_points(repeat, false) {
        out.extend(points);
    }
}

pub(crate) fn entity_points(entity: &Entity, out: &mut Vec<Point>) {
    match bare(entity) {
        Entity::Repeat(repeat) => repeat_points(repeat, out),
        Entity::Line { start, end } => out.extend([*start, *end]),
        Entity::Circle { center, radius } | Entity::Arc { center, radius, .. } => out.extend([
            Point {
                x: center.x - radius,
                y: center.y - radius,
            },
            Point {
                x: center.x + radius,
                y: center.y + radius,
            },
        ]),
        Entity::Point { origin } | Entity::Text { origin, .. } | Entity::Shape { origin, .. } => {
            out.push(*origin)
        }
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            out.extend([*p1, *p2, *p3, *p4])
        }
        Entity::Insert { .. }
        | Entity::Load { .. }
        | Entity::OnLayer { .. }
        | Entity::Erased(_)
        | Entity::Generic(_) => {}
        Entity::Extension(ext) => {
            if let Some(extents) = ext.bounding_extents() {
                out.extend([
                    Point {
                        x: extents.xmin,
                        y: extents.ymin,
                    },
                    Point {
                        x: extents.xmax,
                        y: extents.ymax,
                    },
                ]);
            }
        }
    }
}

pub(crate) fn rotate_point(point: Point, base: Point, degrees: f64) -> Point {
    let angle = degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let x = point.x - base.x;
    let y = point.y - base.y;
    Point {
        x: base.x + x * cos - y * sin,
        y: base.y + x * sin + y * cos,
    }
}

pub(crate) fn trace_quads(points: &[Point], width: f64) -> Result<Vec<Entity>, String> {
    if points.len() < 2 || !width.is_finite() || width <= 0.0 {
        return Err("TRACE needs two points and a positive width".into());
    }
    let directions: Vec<_> = points
        .windows(2)
        .map(|pair| {
            let dx = pair[1].x - pair[0].x;
            let dy = pair[1].y - pair[0].y;
            let length = dx.hypot(dy);
            if !length.is_finite() || length == 0.0 {
                Err("TRACE needs distinct finite points".to_owned())
            } else {
                Ok((dx / length, dy / length))
            }
        })
        .collect::<Result<_, _>>()?;
    let normals: Vec<_> = directions.iter().map(|&(x, y)| (-y, x)).collect();
    let half = width / 2.0;
    let mut edges = Vec::with_capacity(points.len());
    for (index, center) in points.iter().enumerate() {
        let (nx, ny, factor) = if index == 0 {
            (normals[0].0, normals[0].1, half)
        } else if index + 1 == points.len() {
            let normal = normals[index - 1];
            (normal.0, normal.1, half)
        } else {
            let previous = directions[index - 1];
            let next = directions[index];
            let denom = 1.0 + previous.0 * next.0 + previous.1 * next.1;
            if denom <= 1e-9 {
                return Err("TRACE cannot miter a reversing path".into());
            }
            (
                normals[index - 1].0 + normals[index].0,
                normals[index - 1].1 + normals[index].1,
                half / denom,
            )
        };
        let x = nx * factor;
        let y = ny * factor;
        let left = Point {
            x: center.x + x,
            y: center.y + y,
        };
        let right = Point {
            x: center.x - x,
            y: center.y - y,
        };
        if ![left.x, left.y, right.x, right.y]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err("TRACE corner exceeds the finite numeric range".into());
        }
        edges.push((left, right));
    }
    Ok(edges
        .windows(2)
        .map(|pair| Entity::Trace {
            p1: pair[0].0,
            p2: pair[0].1,
            p3: pair[1].0,
            p4: pair[1].1,
        })
        .collect())
}

mod metrics;
pub(crate) use metrics::{area_perimeter, polygon_metrics};

pub(crate) fn assign_layer(entity: &mut Entity, layer: u8) {
    if let Entity::OnLayer { layer: current, .. } = entity {
        *current = layer;
    } else {
        *entity = Entity::OnLayer {
            layer,
            entity: Box::new(entity.clone()),
        };
    }
}

pub(crate) fn line_points(entity: &Entity) -> Option<(Point, Point)> {
    match entity {
        Entity::OnLayer { entity, .. } => line_points(entity),
        Entity::Line { start, end } => Some((*start, *end)),
        _ => None,
    }
}

pub(crate) fn entity_layer(entity: &Entity) -> u8 {
    match entity {
        Entity::OnLayer { layer, .. } => *layer,
        _ => 1,
    }
}

/// What BREAK keeps: the piece on the start side (which keeps the source
/// record) and the piece on the end side (appended). Neither remains when the
/// span covers the whole object, which erases the record.
pub(crate) struct BreakGeometry {
    pub(crate) start: Option<Entity>,
    pub(crate) end: Option<Entity>,
}

mod editing;
pub(crate) use editing::{break_entity_geometry, set_line_points};
