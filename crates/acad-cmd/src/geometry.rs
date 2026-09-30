//! Pure geometry helpers: hatch, dimension, break, fillet and area/perimeter math.

use crate::entity_ops::bare;
use crate::parse::format_measurement;
use crate::MAX_ARRAY_ENTITIES;
use acad_model::{Drawing, Entity, Item, Point, Units};
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(crate) enum HatchEdge {
    Line(Point, Point),
    Arc {
        center: Point,
        radius: f64,
        start_deg: f64,
        end_deg: f64,
    },
}

pub(crate) fn hatch_sweep_deg(start: f64, end: f64) -> f64 {
    let sweep = (end - start).rem_euclid(360.0);
    if sweep == 0.0 {
        360.0
    } else {
        sweep
    }
}

pub(crate) fn hatch_line_geometry(
    drawing: &Drawing,
    ids: &[usize],
    scale: f64,
    angle_deg: f64,
) -> Result<Vec<Entity>, String> {
    let selected: BTreeSet<_> = ids.iter().copied().collect();
    let mut edges = Vec::new();
    let mut circles = Vec::new();
    let mut selectable_id = 0;
    for item in &drawing.items {
        let Item::Entity(entity) = item else {
            continue;
        };
        if matches!(bare(entity), Entity::Load { .. }) {
            continue;
        }
        selectable_id += 1;
        if !selected.contains(&selectable_id) {
            continue;
        }
        match bare(entity) {
            Entity::Line { start, end } => edges.push(HatchEdge::Line(*start, *end)),
            Entity::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            } if *radius > 0.0 => {
                edges.push(HatchEdge::Arc {
                    center: *center,
                    radius: *radius,
                    start_deg: *start_deg,
                    end_deg: *end_deg,
                });
            }
            Entity::Circle { center, radius } if *radius > 0.0 => {
                circles.push((*center, *radius));
            }
            _ => {
                return Err(
                    "HATCH LINE supports closed LINE/ARC loops and CIRCLE boundaries".into(),
                )
            }
        }
    }
    if edges.is_empty() && circles.is_empty() {
        return Err("HATCH needs at least one selected boundary object".into());
    }

    let close = |a: Point, b: Point| (a.x - b.x).abs() <= 1e-8 && (a.y - b.y).abs() <= 1e-8;
    let edge_ends = |edge: HatchEdge| -> (Point, Point) {
        match edge {
            HatchEdge::Line(a, b) => (a, b),
            HatchEdge::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            } => {
                let point = |angle: f64| {
                    let angle = angle.to_radians();
                    Point {
                        x: center.x + radius * angle.cos(),
                        y: center.y + radius * angle.sin(),
                    }
                };
                (point(start_deg), point(end_deg))
            }
        }
    };
    let mut unused = vec![true; edges.len()];
    let mut loops = Vec::<Vec<HatchEdge>>::new();
    while let Some(first_edge) = unused.iter().position(|is_unused| *is_unused) {
        unused[first_edge] = false;
        let (start, next) = edge_ends(edges[first_edge]);
        let mut loop_edges = vec![edges[first_edge]];
        let mut current = next;
        while !close(current, start) {
            let candidates: Vec<_> = edges
                .iter()
                .enumerate()
                .filter(|(index, _)| unused[*index])
                .filter_map(|(index, edge)| {
                    let (a, b) = edge_ends(*edge);
                    if close(a, current) {
                        Some((index, b, false))
                    } else if close(b, current) {
                        Some((index, a, true))
                    } else {
                        None
                    }
                })
                .collect();
            if candidates.len() != 1 {
                return Err("HATCH boundary lines must form closed, unbranched loops".into());
            }
            let (edge, point, reverse) = candidates[0];
            unused[edge] = false;
            let _ = reverse; // Topology is undirected; retain the arc's CCW geometry.
            loop_edges.push(edges[edge]);
            current = point;
            if loop_edges.len() > edges.len() {
                return Err("HATCH boundary loop did not close".into());
            }
        }
        if loop_edges.len() < 2 {
            return Err("HATCH boundary loop needs at least two edges".into());
        }
        loops.push(loop_edges);
    }

    let spacing = 0.125 * scale;
    if !spacing.is_finite() || spacing <= 0.0 {
        return Err("HATCH line spacing is outside the supported range".into());
    }
    let radians = angle_deg.to_radians();
    let direction = Point {
        x: radians.cos(),
        y: radians.sin(),
    };
    let normal = Point {
        x: -direction.y,
        y: direction.x,
    };
    let project = |point: Point, axis: Point| point.x * axis.x + point.y * axis.y;
    let edge_projection_extrema = |edge: HatchEdge| -> (f64, f64) {
        let (a, b) = edge_ends(edge);
        let mut values = vec![project(a, normal), project(b, normal)];
        if let HatchEdge::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } = edge
        {
            let sweep = hatch_sweep_deg(start_deg, end_deg);
            let normal_angle = normal.y.atan2(normal.x).to_degrees();
            for angle in [normal_angle, normal_angle + 180.0] {
                if (angle - start_deg).rem_euclid(360.0) <= sweep + 1e-10 {
                    let radians = angle.to_radians();
                    values.push(project(
                        Point {
                            x: center.x + radius * radians.cos(),
                            y: center.y + radius * radians.sin(),
                        },
                        normal,
                    ));
                }
            }
        }
        (
            values.iter().copied().fold(f64::INFINITY, f64::min),
            values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let edge_extrema: Vec<_> = loops
        .iter()
        .flatten()
        .map(|edge| edge_projection_extrema(*edge))
        .collect();
    let line_min = edge_extrema
        .iter()
        .map(|(min, _)| *min)
        .fold(f64::INFINITY, f64::min);
    let line_max = edge_extrema
        .iter()
        .map(|(_, max)| *max)
        .fold(f64::NEG_INFINITY, f64::max);
    let circle_min = circles
        .iter()
        .map(|(center, radius)| project(*center, normal) - radius)
        .fold(f64::INFINITY, f64::min);
    let circle_max = circles
        .iter()
        .map(|(center, radius)| project(*center, normal) + radius)
        .fold(f64::NEG_INFINITY, f64::max);
    let min = line_min.min(circle_min);
    let max = line_max.max(circle_max);
    let center = (min + max) / 2.0;
    let center_index = (center / spacing).round() as i64;
    let first_index = if center_index as f64 * spacing >= max {
        ((max - 1e-10) / spacing).floor() as i64
    } else if center_index as f64 * spacing < min {
        (min / spacing).ceil() as i64
    } else {
        center_index
    };
    let count = ((max - min) / spacing).ceil().max(0.0) as usize;
    if count > MAX_ARRAY_ENTITIES {
        return Err(format!(
            "HATCH would create {count} lines; limit is {MAX_ARRAY_ENTITIES}"
        ));
    }
    let mut offsets = Vec::with_capacity(count);
    for (direction, mut index) in [(1_i64, first_index), (-1_i64, first_index - 1)] {
        while {
            let offset = index as f64 * spacing;
            offset >= min - 1e-10 && offset < max - 1e-10
        } {
            offsets.push(index as f64 * spacing);
            index += direction;
        }
    }
    let mut lines = Vec::with_capacity(count);
    for offset in offsets {
        let mut intersections = Vec::new();
        for edge in loops.iter().flatten() {
            match *edge {
                HatchEdge::Line(a, b) => {
                    let da = project(a, normal);
                    let db = project(b, normal);
                    if (da <= offset && offset < db) || (db <= offset && offset < da) {
                        let fraction = (offset - da) / (db - da);
                        let crossing = Point {
                            x: a.x + (b.x - a.x) * fraction,
                            y: a.y + (b.y - a.y) * fraction,
                        };
                        intersections.push(project(crossing, direction));
                    }
                }
                HatchEdge::Arc {
                    center,
                    radius,
                    start_deg,
                    end_deg,
                } => {
                    let center_offset = project(center, normal);
                    let delta = offset - center_offset;
                    if delta.abs() < radius {
                        let half_chord = (radius * radius - delta * delta).sqrt();
                        let center_along = project(center, direction);
                        for along in [center_along - half_chord, center_along + half_chord] {
                            let point = Point {
                                x: direction.x * along + normal.x * offset,
                                y: direction.y * along + normal.y * offset,
                            };
                            let angle = (point.y - center.y)
                                .atan2(point.x - center.x)
                                .to_degrees()
                                .rem_euclid(360.0);
                            if (angle - start_deg).rem_euclid(360.0)
                                <= hatch_sweep_deg(start_deg, end_deg) + 1e-9
                            {
                                intersections.push(along);
                            }
                        }
                    }
                }
            }
        }
        for (center, radius) in &circles {
            let center_offset = project(*center, normal);
            let delta = offset - center_offset;
            if delta.abs() < *radius {
                let half_chord = (radius * radius - delta * delta).sqrt();
                let center_along = project(*center, direction);
                intersections.push(center_along - half_chord);
                intersections.push(center_along + half_chord);
            }
        }
        intersections.sort_by(f64::total_cmp);
        intersections.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
        for pair in intersections.chunks_exact(2) {
            let point = |along: f64| Point {
                x: direction.x * along + normal.x * offset,
                y: direction.y * along + normal.y * offset,
            };
            lines.push(Entity::Line {
                start: point(pair[0]),
                end: point(pair[1]),
            });
        }
    }
    Ok(lines)
}

pub(crate) fn dimension_geometry(
    first: Point,
    intersection: Point,
    second: Point,
    text: Option<&str>,
    configured_text_size: f64,
    units: Units,
) -> Result<Vec<Entity>, String> {
    let dx = intersection.x - first.x;
    let dy = intersection.y - first.y;
    let baseline = dx.hypot(dy);
    if !baseline.is_finite() || baseline == 0.0 {
        return Err("DIM extension line points must be distinct".into());
    }
    let along_extension = Point {
        x: dx / baseline,
        y: dy / baseline,
    };
    let normal = Point {
        x: along_extension.y,
        y: -along_extension.x,
    };
    let offset = (second.x - intersection.x) * normal.x + (second.y - intersection.y) * normal.y;
    if !offset.is_finite() || offset == 0.0 {
        return Err("DIM extension lines must not be collinear".into());
    }
    let dimension_length = offset.abs();
    let dimension_direction = Point {
        x: normal.x * offset.signum(),
        y: normal.y * offset.signum(),
    };
    let dimension_end = Point {
        x: intersection.x + normal.x * offset,
        y: intersection.y + normal.y * offset,
    };
    let dimension_mid = Point {
        x: (intersection.x + dimension_end.x) / 2.0,
        y: (intersection.y + dimension_end.y) / 2.0,
    };

    const ARROW: f64 = 9.0 / 64.0;
    const ARROW_HALF_WIDTH: f64 = 3.0 / 128.0;
    let text_height = ((configured_text_size * 135.0).round() / 128.0).max(1.0 / 128.0);
    let dimension_text = text
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format_measurement(dimension_length, units));
    // Native simplex numeric advances observed in QEMU are 52/63 cap heights,
    // with the narrow "1" glyph 40/63. This controls the dimension-line gap;
    // the TEXT record remains the portable glyph data.
    let text_width = dimension_text
        .chars()
        .map(|character| {
            text_height
                * if character == '1' {
                    40.0 / 63.0
                } else {
                    52.0 / 63.0
                }
        })
        .sum::<f64>();
    let line_fits = dimension_length > text_width + 4.0 * ARROW;
    let snap = |value: f64| (value * 128.0).round() / 128.0;
    let snap_point = |point: Point| Point {
        x: snap(point.x),
        y: snap(point.y),
    };
    let point_along = |origin: Point, direction: Point, distance: f64| Point {
        x: origin.x + direction.x * distance,
        y: origin.y + direction.y * distance,
    };
    let mut entities = Vec::with_capacity(7);

    entities.push(Entity::Line {
        start: first,
        end: snap_point(point_along(intersection, along_extension, ARROW)),
    });
    entities.push(Entity::Line {
        start: second,
        end: snap_point(point_along(dimension_end, along_extension, ARROW)),
    });

    let arrow_base_start;
    let arrow_base_end;
    if line_fits {
        arrow_base_start = point_along(intersection, dimension_direction, ARROW);
        arrow_base_end = point_along(dimension_end, dimension_direction, -ARROW);
        let gap_half = text_width / 2.0 + ARROW;
        let left_text_edge = point_along(dimension_mid, dimension_direction, -gap_half);
        let right_text_edge = point_along(dimension_mid, dimension_direction, gap_half);
        if dimension_length > text_width + 4.0 * ARROW {
            entities.push(Entity::Line {
                start: snap_point(arrow_base_start),
                end: left_text_edge,
            });
            entities.push(Entity::Line {
                start: snap_point(arrow_base_end),
                end: right_text_edge,
            });
        }
    } else {
        arrow_base_start = point_along(intersection, dimension_direction, -ARROW);
        arrow_base_end = point_along(dimension_end, dimension_direction, ARROW);
        entities.push(Entity::Line {
            start: snap_point(arrow_base_end),
            end: snap_point(point_along(dimension_end, dimension_direction, 2.0 * ARROW)),
        });
        entities.push(Entity::Line {
            start: snap_point(arrow_base_start),
            end: snap_point(point_along(intersection, dimension_direction, -2.0 * ARROW)),
        });
    }

    let arrow_tips = if line_fits {
        [
            (intersection, arrow_base_start),
            (dimension_end, arrow_base_end),
        ]
    } else {
        [
            (dimension_end, arrow_base_end),
            (intersection, arrow_base_start),
        ]
    };
    for (tip, base) in arrow_tips {
        let side = Point {
            x: dimension_direction.y,
            y: -dimension_direction.x,
        };
        let edge_a = snap_point(Point {
            x: base.x + side.x * ARROW_HALF_WIDTH,
            y: base.y + side.y * ARROW_HALF_WIDTH,
        });
        let edge_b = snap_point(Point {
            x: base.x - side.x * ARROW_HALF_WIDTH,
            y: base.y - side.y * ARROW_HALF_WIDTH,
        });
        let tip = snap_point(tip);
        entities.push(Entity::Solid {
            p1: edge_a,
            p2: edge_b,
            p3: tip,
            p4: tip,
        });
    }

    let text_origin = if line_fits {
        Point {
            x: dimension_mid.x - text_width / 2.0,
            y: dimension_mid.y - text_height / 2.0,
        }
    } else {
        Point {
            x: dimension_mid.x - text_width / 2.0,
            y: dimension_end.y + 2.0 * text_height,
        }
    };
    entities.push(Entity::Text {
        origin: text_origin,
        height: text_height,
        rotation_deg: 0.0,
        value: dimension_text,
    });
    Ok(entities)
}

pub(crate) fn repeat_points(repeat: &acad_model::Repeat, out: &mut Vec<Point>) {
    let mut base = Vec::new();
    for entity in &repeat.entities {
        entity_points(entity, &mut base);
    }
    for row in 0..repeat.rows {
        for column in 0..repeat.columns {
            let dx = f64::from(column) * repeat.column_spacing;
            let dy = f64::from(row) * repeat.row_spacing;
            out.extend(base.iter().map(|p| Point {
                x: p.x + dx,
                y: p.y + dy,
            }));
        }
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
        Entity::Insert { .. } | Entity::Load { .. } | Entity::OnLayer { .. } => {}
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

pub(crate) fn area_perimeter(entities: &[&Entity]) -> Result<(f64, f64), String> {
    if entities.is_empty() {
        return Err("AREA requires at least one entity".into());
    }
    if entities
        .iter()
        .all(|entity| matches!(bare(entity), Entity::Line { .. }))
    {
        let (area, perimeter) = line_loop_area(entities)?;
        return checked_area_metrics(area, perimeter);
    }

    let mut total_area = 0.0;
    let mut total_perimeter = 0.0;
    for entity in entities {
        match bare(entity) {
            Entity::Circle { center: _, radius } if *radius > 0.0 => {
                total_area += std::f64::consts::PI * radius * radius;
                total_perimeter += std::f64::consts::TAU * radius;
            }
            Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
                let (area, perimeter) = polygon_metrics(&[*p1, *p2, *p3, *p4]);
                total_area += area;
                total_perimeter += perimeter;
            }
            Entity::Circle { .. } => {
                return Err("AREA cannot measure a circle with nonpositive radius".into());
            }
            _ => return Err("AREA supports circles, TRACE/SOLID and closed LINE loops".into()),
        }
    }
    checked_area_metrics(total_area, total_perimeter)
}

pub(crate) fn checked_area_metrics(area: f64, perimeter: f64) -> Result<(f64, f64), String> {
    if !area.is_finite() || !perimeter.is_finite() {
        return Err("AREA result exceeds the finite numeric range".into());
    }
    Ok((area, perimeter))
}

pub(crate) fn line_loop_area(entities: &[&Entity]) -> Result<(f64, f64), String> {
    if entities.len() < 3 {
        return Err("AREA needs at least three lines for a closed loop".into());
    }
    let first = line_points(entities[0]).expect("the caller checked every entity is a line");
    if (first.1.x - first.0.x).hypot(first.1.y - first.0.y) <= 1e-12 {
        return Err("AREA cannot measure a zero-length line".into());
    }
    let mut points = vec![first.0, first.1];
    let mut remaining: Vec<_> = entities[1..]
        .iter()
        .map(|entity| line_points(entity).expect("the caller checked every entity is a line"))
        .collect();
    let coordinate_scale = entities
        .iter()
        .filter_map(|entity| line_points(entity))
        .flat_map(|(start, end)| [start.x.abs(), start.y.abs(), end.x.abs(), end.y.abs()])
        .fold(1.0, f64::max);
    let tolerance = coordinate_scale * 1e-9;
    let mut current = first.1;
    while !remaining.is_empty() {
        let matches: Vec<_> = remaining
            .iter()
            .enumerate()
            .filter_map(|(index, (start, end))| {
                let starts_here = (start.x - current.x).hypot(start.y - current.y) <= tolerance;
                let ends_here = (end.x - current.x).hypot(end.y - current.y) <= tolerance;
                (starts_here || ends_here).then_some((index, starts_here, ends_here))
            })
            .collect();
        if matches.len() != 1 {
            return Err("AREA line selection must form one unbranched closed loop".into());
        }
        let (index, starts_here, ends_here) = matches[0];
        if starts_here && ends_here {
            return Err("AREA cannot measure a zero-length line".into());
        }
        let (start, end) = remaining.remove(index);
        let next = if starts_here { end } else { start };
        points.push(next);
        current = next;
    }
    if (current.x - first.0.x).hypot(current.y - first.0.y) > tolerance {
        return Err("AREA line selection is open".into());
    }
    Ok(polygon_metrics(&points))
}

pub(crate) fn polygon_metrics(points: &[Point]) -> (f64, f64) {
    let pairs = points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len());
    let mut twice_area = 0.0;
    let mut perimeter = 0.0;
    for (a, b) in pairs {
        twice_area += a.x * b.y - b.x * a.y;
        perimeter += (b.x - a.x).hypot(b.y - a.y);
    }
    (twice_area.abs() / 2.0, perimeter)
}

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

pub(crate) struct BreakGeometry {
    pub(crate) primary: Entity,
    pub(crate) secondary: Option<Entity>,
}

pub(crate) fn break_entity_geometry(
    entity: &Entity,
    first: Point,
    second: Point,
) -> Result<BreakGeometry, String> {
    match bare(entity) {
        Entity::Line { start, end } => {
            let parts = break_line_geometry((*start, *end), first, second)?;
            Ok(BreakGeometry {
                primary: Entity::Line {
                    start: parts.first.0,
                    end: parts.first.1,
                },
                secondary: Some(Entity::Line {
                    start: parts.second.0,
                    end: parts.second.1,
                }),
            })
        }
        Entity::Circle { center, radius } => {
            let (start_deg, end_deg) = break_circle_angles(*center, *radius, first, second)?;
            Ok(BreakGeometry {
                primary: Entity::Arc {
                    center: *center,
                    radius: *radius,
                    start_deg,
                    end_deg,
                },
                secondary: None,
            })
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            let (before, after) =
                break_arc_geometry(*center, *radius, *start_deg, *end_deg, first, second)?;
            Ok(BreakGeometry {
                primary: before,
                secondary: after,
            })
        }
        _ => Err("BREAK supports LINE, ARC and CIRCLE entities".into()),
    }
}

pub(crate) fn break_circle_angles(
    center: Point,
    radius: f64,
    first: Point,
    second: Point,
) -> Result<(f64, f64), String> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err("BREAK cannot split a circle with a nonpositive radius".into());
    }
    let angle = |point: Point| {
        (point.y - center.y)
            .atan2(point.x - center.x)
            .to_degrees()
            .rem_euclid(360.0)
    };
    let tolerance = radius.max(1.0) * 1e-8;
    for point in [first, second] {
        let distance = (point.x - center.x).hypot(point.y - center.y);
        if (distance - radius).abs() > tolerance {
            return Err("BREAK points must lie on the selected circle".into());
        }
    }
    let first_deg = angle(first);
    let second_deg = angle(second);
    let removed_sweep = (second_deg - first_deg).rem_euclid(360.0);
    if removed_sweep <= 1e-10 || (360.0 - removed_sweep) <= 1e-10 {
        return Err("BREAK points must be distinct".into());
    }
    // BREAK removes the counter-clockwise arc from the first point to the
    // second; the retained complement starts at the second point.
    Ok((second_deg, first_deg))
}

pub(crate) fn break_arc_geometry(
    center: Point,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
    first: Point,
    second: Point,
) -> Result<(Entity, Option<Entity>), String> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err("BREAK cannot split an arc with a nonpositive radius".into());
    }
    let start = start_deg.rem_euclid(360.0);
    let raw_sweep = (end_deg - start_deg).rem_euclid(360.0);
    let sweep = if raw_sweep == 0.0 { 360.0 } else { raw_sweep };
    let point_angle = |point: Point| {
        (point.y - center.y)
            .atan2(point.x - center.x)
            .to_degrees()
            .rem_euclid(360.0)
    };
    let tolerance = radius.max(1.0) * 1e-8;
    let mut path_positions = Vec::with_capacity(2);
    for point in [first, second] {
        let distance = (point.x - center.x).hypot(point.y - center.y);
        if (distance - radius).abs() > tolerance {
            return Err("BREAK points must lie on the selected arc".into());
        }
        let position = (point_angle(point) - start).rem_euclid(360.0);
        if position <= 1e-8 || position >= sweep - 1e-8 {
            return Err("BREAK points must be inside the selected arc".into());
        }
        path_positions.push(position);
    }
    if (path_positions[0] - path_positions[1]).abs() <= 1e-8 {
        return Err("BREAK points must be distinct".into());
    }
    path_positions.sort_by(f64::total_cmp);
    let angle_at = |position: f64| (start + position).rem_euclid(360.0);
    let make_arc = |start_angle, end_angle| Entity::Arc {
        center,
        radius,
        start_deg: start_angle,
        end_deg: end_angle,
    };
    Ok((
        make_arc(start, angle_at(path_positions[0])),
        Some(make_arc(
            angle_at(path_positions[1]),
            (start + sweep).rem_euclid(360.0),
        )),
    ))
}

pub(crate) struct LineBreakParts {
    pub(crate) first: (Point, Point),
    pub(crate) second: (Point, Point),
}

pub(crate) fn break_line_geometry(
    line: (Point, Point),
    first: Point,
    second: Point,
) -> Result<LineBreakParts, String> {
    let direction = Point {
        x: line.1.x - line.0.x,
        y: line.1.y - line.0.y,
    };
    let length_squared = direction.x * direction.x + direction.y * direction.y;
    if length_squared <= 1e-24 {
        return Err("BREAK cannot split a zero-length line".into());
    }
    let parameter = |point: Point| {
        ((point.x - line.0.x) * direction.x + (point.y - line.0.y) * direction.y) / length_squared
    };
    let project = |t: f64| Point {
        x: line.0.x + direction.x * t,
        y: line.0.y + direction.y * t,
    };
    let tolerance = 1e-8 * length_squared.sqrt().max(1.0);
    let first_t = parameter(first);
    let second_t = parameter(second);
    for (point, t) in [(first, first_t), (second, second_t)] {
        let projection = project(t);
        if (point.x - projection.x).hypot(point.y - projection.y) > tolerance {
            return Err("BREAK points must lie on the selected line".into());
        }
        if !(0.0 < t && t < 1.0) {
            return Err("BREAK points must be inside the selected line".into());
        }
    }
    if (first_t - second_t).abs() <= 1e-10 {
        return Err("BREAK points must be distinct".into());
    }
    let first_break = project(first_t);
    let second_break = project(second_t);
    let (first_break, second_break) = if first_t <= second_t {
        (first_break, second_break)
    } else {
        (second_break, first_break)
    };
    Ok(LineBreakParts {
        first: (line.0, first_break),
        second: (second_break, line.1),
    })
}

pub(crate) fn set_line_points(entity: &mut Entity, start: Point, end: Point) {
    match entity {
        Entity::OnLayer { entity, .. } => set_line_points(entity, start, end),
        Entity::Line {
            start: current_start,
            end: current_end,
        } => {
            *current_start = start;
            *current_end = end;
        }
        _ => unreachable!("FILLET validates the selected entity types first"),
    }
}

pub(crate) struct FilletGeometry {
    pub(crate) first: (Point, Point),
    pub(crate) second: (Point, Point),
    pub(crate) center: Point,
    pub(crate) start_deg: f64,
    pub(crate) end_deg: f64,
}

pub(crate) fn fillet_geometry(
    first: (Point, Point),
    second: (Point, Point),
    radius: f64,
) -> Result<FilletGeometry, String> {
    let sub = |a: Point, b: Point| Point {
        x: a.x - b.x,
        y: a.y - b.y,
    };
    let add = |a: Point, b: Point| Point {
        x: a.x + b.x,
        y: a.y + b.y,
    };
    let scale = |p: Point, factor: f64| Point {
        x: p.x * factor,
        y: p.y * factor,
    };
    let cross = |a: Point, b: Point| a.x * b.y - a.y * b.x;
    let length = |p: Point| p.x.hypot(p.y);
    let first_direction = sub(first.1, first.0);
    let second_direction = sub(second.1, second.0);
    let denominator = cross(first_direction, second_direction);
    let direction_scale = length(first_direction) * length(second_direction);
    if direction_scale == 0.0 || denominator.abs() <= 1e-12 * direction_scale {
        return Err("FILLET lines must be nonzero and nonparallel".into());
    }
    let between_starts = sub(second.0, first.0);
    let along_first = cross(between_starts, second_direction) / denominator;
    let along_second = cross(between_starts, first_direction) / denominator;
    let tolerance = 1e-10;
    if !(-tolerance..=1.0 + tolerance).contains(&along_first)
        || !(-tolerance..=1.0 + tolerance).contains(&along_second)
    {
        return Err("FILLET requires the line segments to intersect".into());
    }
    let vertex = add(first.0, scale(first_direction, along_first));
    // AutoCAD chooses the first line's forward ray and the second line's
    // backward ray for this selection order. The picked lines retain those
    // endpoint sides; trim the opposite endpoints to the tangent points.
    let first_ray = first_direction;
    let second_ray = scale(second_direction, -1.0);
    let first_available = length(sub(first.1, vertex));
    let second_available = length(sub(second.0, vertex));
    if first_available <= 1e-12 || second_available <= 1e-12 {
        return Err("FILLET has no room on the selected line rays".into());
    }
    let first_unit = scale(first_ray, 1.0 / length(first_ray));
    let second_unit = scale(second_ray, 1.0 / length(second_ray));
    let dot = (first_unit.x * second_unit.x + first_unit.y * second_unit.y).clamp(-1.0, 1.0);
    let angle = dot.acos();
    if angle <= 1e-10 || (std::f64::consts::PI - angle) <= 1e-10 {
        return Err("FILLET cannot round a zero or straight angle".into());
    }
    let tangent_distance = radius / (angle / 2.0).tan();
    if tangent_distance >= first_available || tangent_distance >= second_available {
        return Err("FILLET radius is too large for the selected lines".into());
    }
    let first_tangent = add(vertex, scale(first_unit, tangent_distance));
    let second_tangent = add(vertex, scale(second_unit, tangent_distance));
    let bisector = add(first_unit, second_unit);
    let center = add(
        vertex,
        scale(bisector, radius / ((angle / 2.0).sin() * length(bisector))),
    );
    let degrees = |point: Point| {
        let angle = (point.y - center.y)
            .atan2(point.x - center.x)
            .to_degrees()
            .rem_euclid(360.0);
        if angle > 360.0 - 1e-10 {
            0.0
        } else {
            angle
        }
    };
    let (start_angle, end_angle) = if cross(first_unit, second_unit) > 0.0 {
        (degrees(second_tangent), degrees(first_tangent))
    } else {
        (degrees(first_tangent), degrees(second_tangent))
    };
    Ok(FilletGeometry {
        first: (first_tangent, first.1),
        second: (second.0, second_tangent),
        center,
        start_deg: start_angle,
        end_deg: end_angle,
    })
}

pub(crate) fn three_point_arc(
    a: Point,
    b: Point,
    c: Point,
) -> Result<(Point, f64, f64, f64), String> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return Err("ARC points are collinear".into());
    }
    let aa = a.x * a.x + a.y * a.y;
    let bb = b.x * b.x + b.y * b.y;
    let cc = c.x * c.x + c.y * c.y;
    let center = Point {
        x: (aa * (b.y - c.y) + bb * (c.y - a.y) + cc * (a.y - b.y)) / d,
        y: (aa * (c.x - b.x) + bb * (a.x - c.x) + cc * (b.x - a.x)) / d,
    };
    let radius = (a.x - center.x).hypot(a.y - center.y);
    let angle = |p: Point| {
        (p.y - center.y)
            .atan2(p.x - center.x)
            .to_degrees()
            .rem_euclid(360.0)
    };
    let (mut start, mut end) = (angle(a), angle(c));
    let middle = angle(b);
    let sweep = (end - start).rem_euclid(360.0);
    if (middle - start).rem_euclid(360.0) > sweep {
        std::mem::swap(&mut start, &mut end);
    }
    Ok((center, radius, start, end))
}
