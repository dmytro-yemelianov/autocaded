use crate::viewport::Viewport;
use acad_model::{Drawing, Entity, Point};

#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Polyline(Vec<Point>),
}

/// Counter-clockwise sweep from `start` to `end`, in degrees, always positive.
/// AutoCAD 1.4 stores arcs CCW, so an end angle below the start wraps through 0.
pub fn sweep_deg(start: f64, end: f64) -> f64 {
    let d = (end - start).rem_euclid(360.0);
    if d == 0.0 {
        360.0
    } else {
        d
    }
}

fn arc_points(c: Point, r: f64, start: f64, sweep: f64, vp: &Viewport) -> Vec<Point> {
    let steps = ((sweep / 4.0).ceil() as usize).max(8);
    (0..=steps)
        .map(|i| {
            let a = (start + sweep * (i as f64 / steps as f64)).to_radians();
            vp.to_screen(Point {
                x: c.x + r * a.cos(),
                y: c.y + r * a.sin(),
            })
        })
        .collect()
}

pub fn flatten_entity(e: &Entity, vp: &Viewport) -> Vec<Prim> {
    match e {
        Entity::Line { start, end } => vec![Prim::Polyline(vec![
            vp.to_screen(*start),
            vp.to_screen(*end),
        ])],
        Entity::Circle { center, radius } => {
            vec![Prim::Polyline(arc_points(*center, *radius, 0.0, 360.0, vp))]
        }
        Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        } => vec![Prim::Polyline(arc_points(
            *center,
            *radius,
            *start_deg,
            sweep_deg(*start_deg, *end_deg),
            vp,
        ))],
        // TEXT needs the .SHP font files, which milestone 1 does not decode.
        Entity::Text { .. } => Vec::new(),
        Entity::Insert { .. } => Vec::new(), // expanded by `flatten`, which has the blocks
    }
}

pub fn flatten(d: &Drawing, vp: &Viewport) -> Vec<Prim> {
    let mut out = Vec::new();
    for e in d.entities() {
        match e {
            Entity::Insert {
                origin,
                x_scale,
                y_scale,
                rotation_deg,
                name,
            } => {
                let Some(b) = d.block(name) else { continue };
                let (sin, cos) = rotation_deg.to_radians().sin_cos();
                for inner in &b.entities {
                    for prim in flatten_entity(inner, vp) {
                        let Prim::Polyline(pts) = prim;
                        // Re-place in world space, then re-project.
                        out.push(Prim::Polyline(
                            pts.into_iter()
                                .map(|sp| {
                                    let w = vp.to_world(sp);
                                    let (dx, dy) = (w.x - b.base.x, w.y - b.base.y);
                                    let (sx, sy) = (dx * x_scale, dy * y_scale);
                                    vp.to_screen(Point {
                                        x: origin.x + sx * cos - sy * sin,
                                        y: origin.y + sx * sin + sy * cos,
                                    })
                                })
                                .collect(),
                        ));
                    }
                }
            }
            other => out.extend(flatten_entity(other, vp)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use acad_model::{Entity, Extents, Point};

    fn vp() -> Viewport {
        Viewport::fit(
            &Extents {
                xmin: 0.0,
                xmax: 10.0,
                ymin: 0.0,
                ymax: 10.0,
            },
            100,
            100,
        )
    }

    #[test]
    fn line_becomes_a_two_point_polyline() {
        let prims = flatten_entity(
            &Entity::Line {
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 10.0, y: 10.0 },
            },
            &vp(),
        );
        let Prim::Polyline(pts) = &prims[0];
        assert_eq!(pts.len(), 2);
    }

    #[test]
    fn circle_is_closed() {
        let prims = flatten_entity(
            &Entity::Circle {
                center: Point { x: 5.0, y: 5.0 },
                radius: 2.0,
            },
            &vp(),
        );
        let Prim::Polyline(pts) = &prims[0];
        let (first, last) = (pts[0], *pts.last().unwrap());
        assert!((first.x - last.x).abs() < 1e-9 && (first.y - last.y).abs() < 1e-9);
    }

    #[test]
    fn arc_sweeps_counter_clockwise_through_zero() {
        // SUBDIV's arc: start 78.6041 deg, end 22.4509 deg.
        // CCW from 78.6 to 22.45 wraps through 360/0, a sweep of 303.847 deg.
        assert!((sweep_deg(78.6041, 22.4509) - 303.8468).abs() < 1e-3);
    }

    #[test]
    fn a_full_turn_is_used_when_start_equals_end() {
        assert!((sweep_deg(90.0, 90.0) - 360.0).abs() < 1e-9);
    }

    #[test]
    fn text_and_bare_insert_produce_nothing_yet() {
        // .SHP font decoding is milestone 5; a bare INSERT is expanded by
        // `flatten`, which owns the block table, not by `flatten_entity`.
        assert!(flatten_entity(
            &Entity::Text {
                origin: Point { x: 0.0, y: 0.0 },
                height: 1.0,
                rotation_deg: 0.0,
                value: "A".into()
            },
            &vp()
        )
        .is_empty());
    }
}
