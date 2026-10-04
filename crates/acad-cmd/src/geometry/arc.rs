//! ARC construction. Signed angles travel CW/CCW; records remain CCW.
use super::circle;
use crate::{curve_history::Tangent, parse::sin_cos_degrees};
use acad_model::{Entity, Point};

pub(crate) struct ArcGeometry {
    pub(crate) entity: Entity,
    pub(crate) tangent: Tangent,
}
fn angle(p: Point, c: Point) -> f64 {
    let value = (p.y - c.y).atan2(p.x - c.x).to_degrees().rem_euclid(360.0);
    // rem_euclid can round a tiny negative angle up to the modulus.
    if value == 360.0 {
        0.0
    } else {
        value
    }
}
fn signed_angle(degrees: f64) -> Result<f64, String> {
    if !degrees.is_finite() || degrees == 0.0 || degrees.abs() >= 360.0 {
        return Err("ARC angle must be nonzero and less than 360 degrees in magnitude".into());
    }
    Ok(degrees)
}
fn finish(start: Point, center: Point, end: Point, clockwise: bool) -> Result<ArcGeometry, String> {
    let radius = (start.x - center.x).hypot(start.y - center.y);
    circle::validate(center, radius).map_err(|e| e.replace("CIRCLE", "ARC"))?;
    let end_radius = (end.x - center.x).hypot(end.y - center.y);
    if !end_radius.is_finite() || end_radius <= 0.0 {
        return Err("ARC end point is invalid".into());
    }
    let (a, b) = (angle(start, center), angle(end, center));
    if a == b {
        return Err("ARC start and end directions must differ".into());
    }
    let radial = Point {
        x: (end.x - center.x) / end_radius,
        y: (end.y - center.y) / end_radius,
    };
    Ok(ArcGeometry {
        entity: Entity::Arc {
            center,
            radius,
            start_deg: if clockwise { b } else { a },
            end_deg: if clockwise { a } else { b },
        },
        tangent: Tangent {
            end,
            direction: if clockwise {
                Point {
                    x: radial.y,
                    y: -radial.x,
                }
            } else {
                Point {
                    x: -radial.y,
                    y: radial.x,
                }
            },
        },
    })
}
pub(crate) fn center_end(
    start: Point,
    center: Point,
    designated: Point,
) -> Result<ArcGeometry, String> {
    let radius = (start.x - center.x).hypot(start.y - center.y);
    let distance = (designated.x - center.x).hypot(designated.y - center.y);
    if !distance.is_finite() || distance == 0.0 {
        return Err("ARC end direction needs a point distinct from its center".into());
    }
    let end = if radius == distance {
        designated
    } else {
        Point {
            x: center.x + (designated.x - center.x) / distance * radius,
            y: center.y + (designated.y - center.y) / distance * radius,
        }
    };
    finish(start, center, end, false)
}
pub(crate) fn center_angle(
    start: Point,
    center: Point,
    degrees: f64,
) -> Result<ArcGeometry, String> {
    let degrees = signed_angle(degrees)?;
    let (sin, cos) = sin_cos_degrees(degrees);
    let dx = start.x - center.x;
    let dy = start.y - center.y;
    finish(
        start,
        center,
        Point {
            x: center.x + dx * cos - dy * sin,
            y: center.y + dx * sin + dy * cos,
        },
        degrees < 0.0,
    )
}
pub(crate) fn center_chord(start: Point, center: Point, chord: f64) -> Result<ArcGeometry, String> {
    let radius = (start.x - center.x).hypot(start.y - center.y);
    let ratio = (chord.abs() * 0.5) / radius;
    if !chord.is_finite() || chord == 0.0 || !ratio.is_finite() || ratio <= 0.0 || ratio > 1.0 {
        return Err("ARC chord must be nonzero and at most the diameter in magnitude".into());
    }
    let minor = 2.0 * ratio.asin().to_degrees();
    center_angle(
        start,
        center,
        if chord < 0.0 { 360.0 - minor } else { minor },
    )
}
fn half_chord(start: Point, end: Point) -> Result<(Point, Point, f64), String> {
    let midpoint = Point {
        x: start.x * 0.5 + end.x * 0.5,
        y: start.y * 0.5 + end.y * 0.5,
    };
    let dx = end.x * 0.5 - start.x * 0.5;
    let dy = end.y * 0.5 - start.y * 0.5;
    let half = dx.hypot(dy);
    if !half.is_finite() || half <= 0.0 {
        return Err("ARC needs distinct finite endpoints".into());
    }
    Ok((
        midpoint,
        Point {
            x: -dy / half,
            y: dx / half,
        },
        half,
    ))
}
pub(crate) fn end_radius(start: Point, end: Point, radius: f64) -> Result<ArcGeometry, String> {
    let (mid, n, half) = half_chord(start, end)?;
    let size = radius.abs();
    if !size.is_finite() || size < half {
        return Err("ARC radius must span its chord".into());
    }
    let ratio = half / size;
    let height =
        size * ((1.0 - ratio) * (1.0 + ratio)).sqrt() * if radius < 0.0 { -1.0 } else { 1.0 };
    finish(
        start,
        Point {
            x: mid.x + n.x * height,
            y: mid.y + n.y * height,
        },
        end,
        false,
    )
}
pub(crate) fn end_angle(start: Point, end: Point, degrees: f64) -> Result<ArcGeometry, String> {
    let degrees = signed_angle(degrees)?;
    let (mid, n, half) = half_chord(start, end)?;
    let height = if degrees.abs() == 180.0 {
        0.0
    } else {
        half / (degrees.to_radians() * 0.5).tan()
    };
    finish(
        start,
        Point {
            x: mid.x + n.x * height,
            y: mid.y + n.y * height,
        },
        end,
        degrees < 0.0,
    )
}
pub(crate) fn end_direction(
    start: Point,
    end: Point,
    direction: Point,
) -> Result<ArcGeometry, String> {
    let (_, _, half) = half_chord(start, end)?;
    let norm = direction.x.hypot(direction.y);
    if !norm.is_finite() || norm <= 0.0 {
        return Err("ARC starting direction must be nonzero and finite".into());
    }
    let n = Point {
        x: -direction.y / norm,
        y: direction.x / norm,
    };
    let projection =
        ((end.x * 0.5 - start.x * 0.5) / half) * n.x + ((end.y * 0.5 - start.y * 0.5) / half) * n.y;
    if projection.abs() <= 32.0 * f64::EPSILON {
        return Err("ARC starting direction is parallel to its chord".into());
    }
    let signed_radius = half / projection;
    finish(
        start,
        Point {
            x: start.x + n.x * signed_radius,
            y: start.y + n.y * signed_radius,
        },
        end,
        signed_radius < 0.0,
    )
}
pub(crate) fn three_points(start: Point, middle: Point, end: Point) -> Result<ArcGeometry, String> {
    let (center, _) =
        circle::through_three_points(start, middle, end).map_err(|e| e.replace("CIRCLE", "ARC"))?;
    let a = angle(start, center);
    let m = angle(middle, center);
    let b = angle(end, center);
    finish(
        start,
        center,
        end,
        (m - a).rem_euclid(360.0) > (b - a).rem_euclid(360.0),
    )
}
