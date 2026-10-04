//! BREAK geometry (docs/native-array-break.md) and supporting line helpers.
use super::*;

/// Parameters closer than this to each other or to an end are equal.
const PARAMETER_TOLERANCE: f64 = 1e-10;

pub(crate) fn break_entity_geometry(
    entity: &Entity,
    first: Point,
    second: Point,
) -> Result<BreakGeometry, String> {
    match bare(entity) {
        Entity::Line { start, end } => break_line(*start, *end, first, second),
        Entity::Trace { p1, p2, p3, p4 } => break_trace([*p1, *p2, *p3, *p4], first, second),
        Entity::Circle { center, radius } => {
            let (start_deg, end_deg) = break_circle_angles(*center, *radius, first, second)?;
            Ok(BreakGeometry {
                start: None,
                end: Some(Entity::Arc {
                    center: *center,
                    radius: *radius,
                    start_deg,
                    end_deg,
                }),
            })
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => break_arc(*center, *radius, *start_deg, *end_deg, first, second),
        _ => Err("BREAK needs a LINE, TRACE, CIRCLE or ARC".into()),
    }
}

/// Order two clamped path parameters in `[0, 1]` and decide which pieces
/// remain. Returns `(low, high, keep_start, keep_end)`.
fn break_span(a: f64, b: f64) -> Result<(f64, f64, bool, bool), String> {
    let (low, high) = if a <= b { (a, b) } else { (b, a) };
    if high - low <= PARAMETER_TOLERANCE {
        return Err("BREAK points must be distinct".into());
    }
    let keep_start = low > PARAMETER_TOLERANCE;
    let keep_end = high < 1.0 - PARAMETER_TOLERANCE;
    // A span covering the whole object keeps neither piece: the original
    // erases the record (in-tree anchored for LINE, ARC and TRACE).
    Ok((low, high, keep_start, keep_end))
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    }
}

fn dot(a: Point, b: Point) -> f64 {
    a.x * b.x + a.y * b.y
}

fn sub(a: Point, b: Point) -> Point {
    Point {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

/// Points are projected perpendicularly onto the line and clamped to it, so
/// a point at or beyond an end cuts that end off.
fn break_line(
    start: Point,
    end: Point,
    first: Point,
    second: Point,
) -> Result<BreakGeometry, String> {
    let direction = sub(end, start);
    let length_squared = dot(direction, direction);
    if length_squared.is_nan() || length_squared <= 1e-24 {
        return Err("BREAK cannot split a zero-length line".into());
    }
    let parameter =
        |point: Point| (dot(sub(point, start), direction) / length_squared).clamp(0.0, 1.0);
    let (low, high, keep_start, keep_end) = break_span(parameter(first), parameter(second))?;
    Ok(BreakGeometry {
        start: keep_start.then(|| Entity::Line {
            start,
            end: lerp(start, end, low),
        }),
        end: keep_end.then(|| Entity::Line {
            start: lerp(start, end, high),
            end,
        }),
    })
}

/// TRACE corners follow the TRACE command's winding: `p1/p2` start edge,
/// `p3/p4` end edge, sides `p1 -> p3` and `p2 -> p4`. Cuts are perpendicular
/// to the centerline between the edge midpoints. Other shapes, and cuts that
/// miss a side inside a mitered end, are refused rather than guessed.
fn break_trace(corners: [Point; 4], first: Point, second: Point) -> Result<BreakGeometry, String> {
    let [p1, p2, p3, p4] = corners;
    let unsupported =
        || "BREAK cannot split this TRACE: corners are not in TRACE winding".to_owned();
    let start_mid = lerp(p1, p2, 0.5);
    let end_mid = lerp(p3, p4, 0.5);
    let axis = sub(end_mid, start_mid);
    let length = axis.x.hypot(axis.y);
    if !length.is_finite() || length <= 1e-12 {
        return Err(unsupported());
    }
    let along = Point {
        x: axis.x / length,
        y: axis.y / length,
    };
    let across = Point {
        x: -along.y,
        y: along.x,
    };
    let offset = |point: Point| dot(sub(point, start_mid), across);
    let (o1, o2, o3, o4) = (offset(p1), offset(p2), offset(p3), offset(p4));
    let sides_apart = o1 * o2 < 0.0 && o3 * o4 < 0.0 && o1 * o3 > 0.0;
    let side_a = dot(sub(p3, p1), along);
    let side_b = dot(sub(p4, p2), along);
    let advances = |run: f64| run > 1e-12;
    if !sides_apart || !advances(side_a) || !advances(side_b) {
        return Err(unsupported());
    }
    let cut = |t: f64| -> Result<(Point, Point), String> {
        let distance = t * length;
        let on_side = |from: Point, to: Point, run: f64| {
            let lambda = (distance - dot(sub(from, start_mid), along)) / run;
            (-PARAMETER_TOLERANCE..=1.0 + PARAMETER_TOLERANCE)
                .contains(&lambda)
                .then(|| lerp(from, to, lambda.clamp(0.0, 1.0)))
        };
        match (on_side(p1, p3, side_a), on_side(p2, p4, side_b)) {
            (Some(a), Some(b)) => Ok((a, b)),
            _ => Err("BREAK point falls inside a mitered TRACE end".into()),
        }
    };
    let parameter = |point: Point| (dot(sub(point, start_mid), along) / length).clamp(0.0, 1.0);
    let (low, high, keep_start, keep_end) = break_span(parameter(first), parameter(second))?;
    let start = if keep_start {
        let (a, b) = cut(low)?;
        Some(Entity::Trace {
            p1,
            p2,
            p3: a,
            p4: b,
        })
    } else {
        None
    };
    let end = if keep_end {
        let (a, b) = cut(high)?;
        Some(Entity::Trace {
            p1: a,
            p2: b,
            p3,
            p4,
        })
    } else {
        None
    };
    Ok(BreakGeometry { start, end })
}

fn polar_degrees(center: Point, point: Point) -> Result<f64, String> {
    let delta = sub(point, center);
    if delta.x.hypot(delta.y) <= 1e-12 {
        return Err("BREAK point is at the center".into());
    }
    Ok(delta.y.atan2(delta.x).to_degrees().rem_euclid(360.0))
}

/// Circle points are projected radially; the counterclockwise span from the
/// first point to the second is removed.
pub(crate) fn break_circle_angles(
    center: Point,
    radius: f64,
    first: Point,
    second: Point,
) -> Result<(f64, f64), String> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err("BREAK cannot split a circle with a nonpositive radius".into());
    }
    let first_deg = polar_degrees(center, first)?;
    let second_deg = polar_degrees(center, second)?;
    let removed_sweep = (second_deg - first_deg).rem_euclid(360.0);
    if removed_sweep <= 1e-10 || (360.0 - removed_sweep) <= 1e-10 {
        return Err("BREAK points must be distinct".into());
    }
    // The retained complement starts at the second point.
    Ok((second_deg, first_deg))
}

/// Arc points are projected radially; positions outside the sweep snap to
/// the nearer arc end (a tie goes to the end angle).
fn break_arc(
    center: Point,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
    first: Point,
    second: Point,
) -> Result<BreakGeometry, String> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err("BREAK cannot split an arc with a nonpositive radius".into());
    }
    let start = start_deg.rem_euclid(360.0);
    let raw_sweep = (end_deg - start_deg).rem_euclid(360.0);
    let sweep = if raw_sweep == 0.0 { 360.0 } else { raw_sweep };
    let parameter = |point: Point| -> Result<f64, String> {
        let position = (polar_degrees(center, point)? - start).rem_euclid(360.0);
        Ok(if position <= sweep {
            position / sweep
        } else if position - sweep <= 360.0 - position {
            1.0
        } else {
            0.0
        })
    };
    let (low, high, keep_start, keep_end) = break_span(parameter(first)?, parameter(second)?)?;
    let angle_at = |t: f64| (start + sweep * t).rem_euclid(360.0);
    let arc = |from: f64, to: f64| Entity::Arc {
        center,
        radius,
        start_deg: from,
        end_deg: to,
    };
    Ok(BreakGeometry {
        start: keep_start.then(|| arc(start_deg, angle_at(low))),
        end: keep_end.then(|| arc(angle_at(high), end_deg)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Editor;

    #[test]
    fn mouse_points_enter_line_vertices_through_the_command_state_machine() {
        let mut editor = Editor::default();
        assert!(!editor.accepts_mouse_point());
        editor.submit("LINE").unwrap();
        assert!(editor.accepts_mouse_point());
        editor
            .submit_mouse_point(Point { x: 1.25, y: -2.5 })
            .unwrap();
        editor.submit_mouse_point(Point { x: 8.0, y: 4.0 }).unwrap();
        editor.submit("").unwrap();

        let entities = editor.drawing.entities().collect::<Vec<_>>();
        assert_eq!(entities.len(), 1);
        let (start, end) = line_points(entities[0]).unwrap();
        assert_eq!(start, Point { x: 1.25, y: -2.5 });
        assert_eq!(end, Point { x: 8.0, y: 4.0 });
        assert!(!editor.accepts_mouse_point());
        assert!(editor.submit_mouse_point(Point { x: 0.0, y: 0.0 }).is_err());
    }
}
