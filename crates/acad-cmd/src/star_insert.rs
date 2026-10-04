//! Native scale/rotation for star INSERT (docs/native-external-insert.md).
//! The original's `INSERT *name` never asks for scale or rotation (it returns
//! to `Command:` after the insertion point and rejects any other answer);
//! the `S` keyword at that prompt is a Rust extension. Members are placed by
//! `origin + R(rotation) * diag(x, y) * (p - base)`, the transform the
//! renderer applies to a block INSERT, and a member whose stored fields
//! cannot represent the result is refused before any change.

use crate::entity_ops::{has_repeat_lattice, transform_entity};
use crate::input_state::Transform;
use acad_model::{Entity, Item, Point};

/// Star placement factors; the identity keeps the evidenced translation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    pub(crate) x_scale: f64,
    pub(crate) y_scale: f64,
    pub(crate) rotation_deg: f64,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            x_scale: 1.0,
            y_scale: 1.0,
            rotation_deg: 0.0,
        }
    }
}

/// Exact sine/cosine for quarter turns, so axis-aligned placements stay
/// axis-aligned (REPEAT lattices) and produce no rounding noise.
fn sin_cos(degrees: f64) -> (f64, f64) {
    let turn = degrees.rem_euclid(360.0);
    [
        (0.0, (0.0, 1.0)),
        (90.0, (1.0, 0.0)),
        (180.0, (0.0, -1.0)),
        (270.0, (-1.0, 0.0)),
    ]
    .into_iter()
    .find(|(quarter, _)| turn == *quarter)
    .map_or_else(|| degrees.to_radians().sin_cos(), |(_, exact)| exact)
}

#[derive(Clone, Copy)]
struct Affine {
    /// Row-major 2x2 linear part.
    m: [[f64; 2]; 2],
    base: Point,
    origin: Point,
    placement: Placement,
}

impl Affine {
    fn new(placement: Placement, base: Point, origin: Point) -> Self {
        let (sin, cos) = sin_cos(placement.rotation_deg);
        let (x, y) = (placement.x_scale, placement.y_scale);
        Self {
            m: [[cos * x, -sin * y], [sin * x, cos * y]],
            base,
            origin,
            placement,
        }
    }
    fn vector(&self, v: Point) -> Point {
        Point {
            x: self.m[0][0] * v.x + self.m[0][1] * v.y,
            y: self.m[1][0] * v.x + self.m[1][1] * v.y,
        }
    }
    fn point(&self, p: Point) -> Point {
        let v = self.vector(Point {
            x: p.x - self.base.x,
            y: p.y - self.base.y,
        });
        Point {
            x: self.origin.x + v.x,
            y: self.origin.y + v.y,
        }
    }
    fn uniform(&self) -> bool {
        self.placement.x_scale == self.placement.y_scale
    }
    /// Circles and arcs need a similarity (mirroring allowed).
    fn similar(&self) -> bool {
        self.placement.x_scale.abs() == self.placement.y_scale.abs()
    }
    fn mirrored(&self) -> bool {
        self.placement.x_scale * self.placement.y_scale < 0.0
    }
    fn factor(&self) -> f64 {
        self.placement.x_scale.abs()
    }
    /// Direction of a member angle after placement, in degrees.
    fn angle(&self, degrees: f64) -> f64 {
        if self.uniform() {
            let turn = if self.placement.x_scale < 0.0 {
                180.0
            } else {
                0.0
            };
            return degrees + self.placement.rotation_deg + turn;
        }
        let (sin, cos) = degrees.to_radians().sin_cos();
        let v = self.vector(Point { x: cos, y: sin });
        v.y.atan2(v.x).to_degrees()
    }
    fn axis_aligned(&self) -> bool {
        self.m[0][1] == 0.0 && self.m[1][0] == 0.0
    }
}

fn kind(entity: &Entity) -> &'static str {
    match entity {
        Entity::Circle { .. } => "CIRCLE",
        Entity::Arc { .. } => "ARC",
        Entity::Text { .. } => "TEXT",
        Entity::Shape { .. } => "SHAPE",
        Entity::Insert { .. } => "INSERT",
        _ => "REPEAT",
    }
}

fn unrepresentable(entity: &Entity, why: &str) -> String {
    format!(
        "INSERT *: {why} cannot be applied to a member {}; insert as a block instead",
        kind(entity)
    )
}

/// Place one stored member (any wrapper stack) or refuse it.
fn place(entity: &mut Entity, a: &Affine) -> Result<(), String> {
    let refuse = |entity: &Entity| {
        let why = if a.similar() {
            "mirroring"
        } else {
            "non-uniform scale"
        };
        Err(unrepresentable(entity, why))
    };
    match entity {
        Entity::OnLayer { entity, .. } | Entity::Erased(entity) => place(entity, a)?,
        Entity::Load { .. } => {}
        Entity::Line { start, end } => {
            *start = a.point(*start);
            *end = a.point(*end);
        }
        Entity::Point { origin } => *origin = a.point(*origin),
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            for p in [p1, p2, p3, p4] {
                *p = a.point(*p);
            }
        }
        Entity::Circle { center, radius } => {
            if !a.similar() {
                return refuse(entity);
            }
            *center = a.point(*center);
            *radius *= a.factor();
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => {
            if !a.similar() {
                return refuse(entity);
            }
            *center = a.point(*center);
            *radius *= a.factor();
            let (start, end) = (a.angle(*start_deg), a.angle(*end_deg));
            // A reflection reverses the counter-clockwise sweep.
            let (start, end) = if a.mirrored() {
                (end, start)
            } else {
                (start, end)
            };
            *start_deg = start.rem_euclid(360.0);
            *end_deg = end.rem_euclid(360.0);
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
            if !a.uniform() {
                return refuse(entity);
            }
            *origin = a.point(*origin);
            *height *= a.factor();
            *rotation_deg = a.angle(*rotation_deg).rem_euclid(360.0);
        }
        Entity::Insert {
            origin,
            x_scale,
            y_scale,
            rotation_deg,
            ..
        } => {
            if a.uniform() && a.placement.x_scale > 0.0 {
                *x_scale *= a.factor();
                *y_scale *= a.factor();
                *rotation_deg = a.angle(*rotation_deg).rem_euclid(360.0);
            } else {
                // Combined linear part M * R(phi) * diag(x, y) must keep
                // orthogonal columns to be stored as R(psi) * diag(x', y').
                let (sin, cos) = sin_cos(*rotation_deg);
                let c1 = a.vector(Point {
                    x: cos * *x_scale,
                    y: sin * *x_scale,
                });
                let c2 = a.vector(Point {
                    x: -sin * *y_scale,
                    y: cos * *y_scale,
                });
                let (n1, n2) = (c1.x.hypot(c1.y), c2.x.hypot(c2.y));
                if (c1.x * c2.x + c1.y * c2.y).abs() > 1e-12 * n1 * n2 {
                    return Err(unrepresentable(entity, "this scale and rotation"));
                }
                // Of the two decompositions (X sign kept or flipped), keep
                // the angle nearest the member's turned angle.
                let target = *rotation_deg + a.placement.rotation_deg;
                let candidate = |sign: f64| {
                    let psi = (sign * c1.y).atan2(sign * c1.x).to_degrees();
                    let off = (psi - target).rem_euclid(360.0);
                    (off.min(360.0 - off), sign, psi)
                };
                let (keep, flip) = (candidate(1.0), candidate(-1.0));
                let (_, sign, psi) = if flip.0 < keep.0 { flip } else { keep };
                let (s, c) = psi.to_radians().sin_cos();
                *x_scale = sign * n1;
                *y_scale = -s * c2.x + c * c2.y;
                *rotation_deg = psi.rem_euclid(360.0);
            }
            *origin = a.point(*origin);
        }
        Entity::Repeat(repeat) => {
            if has_repeat_lattice(&Entity::Repeat(repeat.clone())) && !a.axis_aligned() {
                return Err(
                    "INSERT *: a member REPEAT lattice cannot be rotated off its axes; insert as a block instead"
                        .into(),
                );
            }
            for inner in &mut repeat.entities {
                place(inner, a)?;
            }
            if a.axis_aligned() {
                repeat.column_spacing *= a.m[0][0];
                repeat.row_spacing *= a.m[1][1];
            }
        }
    }
    Ok(())
}

fn finite(entity: &Entity) -> bool {
    let mut points = Vec::new();
    crate::geometry::entity_points(entity, &mut points);
    points.iter().all(|p| p.x.is_finite() && p.y.is_finite())
}

/// Copy `members` to drawing-root items placed at `origin`. The identity
/// placement keeps the evidenced exact translation by `origin - base`.
pub(crate) fn place_members<'a>(
    members: impl IntoIterator<Item = &'a Entity>,
    base: Point,
    origin: Point,
    placement: Placement,
) -> Result<Vec<Entity>, String> {
    let identity = placement == Placement::default();
    let affine = Affine::new(placement, base, origin);
    let delta = Point {
        x: origin.x - base.x,
        y: origin.y - base.y,
    };
    members
        .into_iter()
        .map(|member| {
            let mut entity = member.clone();
            if identity {
                transform_entity(&mut entity, Transform::Translate(delta));
            } else {
                place(&mut entity, &affine)?;
                if !finite(&entity) {
                    return Err("INSERT *: placed geometry exceeds the finite range".into());
                }
            }
            Ok(entity)
        })
        .collect()
}

/// Root-item form of [`place_members`] for external star imports.
pub(crate) fn place_items(
    items: &[Item],
    base: Point,
    origin: Point,
    placement: Placement,
) -> Result<Vec<Item>, String> {
    items
        .iter()
        .map(|item| {
            let (entity, erased) = match item {
                Item::Entity(entity) => (entity.clone(), false),
                Item::Erased(entity) => (entity.clone(), true),
                Item::Repeat(repeat) => (Entity::Repeat(repeat.clone()), false),
                Item::Block(_) => unreachable!("star payloads hold root records only"),
            };
            let [placed] =
                <[Entity; 1]>::try_from(place_members([&entity], base, origin, placement)?)
                    .expect("one member");
            Ok(match (placed, erased) {
                (entity, true) => Item::Erased(entity),
                (Entity::Repeat(repeat), false) => Item::Repeat(repeat),
                (entity, false) => Item::Entity(entity),
            })
        })
        .collect()
}

/// Representability check without an origin (used at the rotation prompt).
pub(crate) fn check_members<'a>(
    members: impl IntoIterator<Item = &'a Entity>,
    base: Point,
    placement: Placement,
) -> Result<(), String> {
    place_members(members, base, base, placement).map(|_| ())
}
