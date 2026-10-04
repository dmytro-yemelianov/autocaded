//! Finite CIRCLE construction independent of editor state and file formats.
use acad_model::Point;

pub(crate) fn validate(center: Point, radius: f64) -> Result<(Point, f64), String> {
    if !radius.is_finite()
        || radius <= 0.0
        || ![
            center.x,
            center.y,
            center.x - radius,
            center.x + radius,
            center.y - radius,
            center.y + radius,
        ]
        .iter()
        .all(|n| n.is_finite())
    {
        return Err("CIRCLE requires a positive finite radius and finite bounds".into());
    }
    Ok((center, radius))
}

pub(crate) fn diameter_endpoints(a: Point, b: Point) -> Result<(Point, f64), String> {
    // Halve before adding/subtracting, avoiding overflow for opposite endpoints.
    let center = Point {
        x: a.x * 0.5 + b.x * 0.5,
        y: a.y * 0.5 + b.y * 0.5,
    };
    let radius = (a.x * 0.5 - b.x * 0.5).hypot(a.y * 0.5 - b.y * 0.5);
    validate(center, radius)
}

pub(crate) fn through_three_points(a: Point, b: Point, c: Point) -> Result<(Point, f64), String> {
    let differences = [b.x - a.x, b.y - a.y, c.x - a.x, c.y - a.y];
    let (scale, u, v) = if differences.iter().all(|n| n.is_finite()) {
        let scale = differences.iter().map(|n| n.abs()).fold(0.0, f64::max);
        (
            scale,
            Point {
                x: differences[0] / scale,
                y: differences[1] / scale,
            },
            Point {
                x: differences[2] / scale,
                y: differences[3] / scale,
            },
        )
    } else {
        let scale = [a.x, a.y, b.x, b.y, c.x, c.y]
            .iter()
            .map(|n| n.abs())
            .fold(0.0, f64::max);
        (
            scale,
            Point {
                x: b.x / scale - a.x / scale,
                y: b.y / scale - a.y / scale,
            },
            Point {
                x: c.x / scale - a.x / scale,
                y: c.y / scale - a.y / scale,
            },
        )
    };
    let left = u.x * v.y;
    let right = u.y * v.x;
    let cross = left - right;
    // Relative cancellation bound, rather than an absolute world-unit epsilon.
    if !scale.is_finite()
        || scale <= 0.0
        || !cross.is_finite()
        || cross.abs() <= 32.0 * f64::EPSILON * (left.abs() + right.abs())
    {
        return Err("CIRCLE points are collinear or numerically indistinguishable".into());
    }
    let uu = u.x * u.x + u.y * u.y;
    let vv = v.x * v.x + v.y * v.y;
    let x = (uu * v.y - vv * u.y) / (2.0 * cross);
    let y = (u.x * vv - v.x * uu) / (2.0 * cross);
    validate(
        Point {
            x: a.x + x * scale,
            y: a.y + y * scale,
        },
        x.hypot(y) * scale,
    )
}
