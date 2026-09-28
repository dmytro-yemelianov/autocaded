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
        Entity::Point { origin } => {
            // A bare dot has nothing to stroke a polyline between (`rasterize`
            // drops anything under 2 points), and 1983 AutoCAD has no vector
            // font glyph to reuse here (`PDMODE`-style point styles are a
            // later AutoCAD feature) — so this draws a small screen-space "+"
            // at a fixed pixel size, independent of the drawing's own scale,
            // rather than rendering to nothing (Review Focus 3).
            let p = vp.to_screen(*origin);
            const R: f64 = 3.0;
            vec![
                Prim::Polyline(vec![
                    Point { x: p.x - R, y: p.y },
                    Point { x: p.x + R, y: p.y },
                ]),
                Prim::Polyline(vec![
                    Point { x: p.x, y: p.y - R },
                    Point { x: p.x, y: p.y + R },
                ]),
            ]
        }
        // `TRACE` and `SOLID` are inferred record types (`acad-dwg`'s
        // `entity.rs` module doc: no corpus DXF exercises either, so their
        // four corners' file-order semantics are not verified field by
        // field). AutoCAD stores both in a "Z" order — p1-p2 one edge,
        // p4-p3 the parallel opposite edge — not sequential winding, so the
        // outline must visit p1, p2, p4, p3 to trace the quadrilateral
        // rather than a self-crossing bowtie. That ordering is the
        // well-documented AutoCAD SOLID/TRACE convention, applied here
        // un-verified against this corpus; milestone 1 has no fill, so both
        // draw as an outline only.
        Entity::Trace { p1, p2, p3, p4 } | Entity::Solid { p1, p2, p3, p4 } => {
            vec![Prim::Polyline(vec![
                vp.to_screen(*p1),
                vp.to_screen(*p2),
                vp.to_screen(*p4),
                vp.to_screen(*p3),
                vp.to_screen(*p1),
            ])]
        }
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

    #[test]
    fn point_draws_a_visible_marker_not_nothing() {
        // A single-point polyline is dropped by `rasterize` (needs >= 2
        // points), so a POINT must produce something with real extent or it
        // renders as a silently missing shape (Review Focus 3).
        let prims = flatten_entity(
            &Entity::Point {
                origin: Point { x: 5.0, y: 5.0 },
            },
            &vp(),
        );
        assert!(!prims.is_empty());
        for Prim::Polyline(pts) in &prims {
            assert!(pts.len() >= 2, "every primitive must be drawable");
            let (a, b) = (pts[0], pts[1]);
            assert!(
                (a.x - b.x).abs() > 0.0 || (a.y - b.y).abs() > 0.0,
                "marker must have non-zero extent"
            );
        }
    }

    #[test]
    fn trace_and_solid_close_into_a_quadrilateral_outline() {
        // Corners stored in AutoCAD's "Z" order (p1-p2 one edge, p4-p3 the
        // opposite edge): drawn as p1,p2,p4,p3,p1 this closes a simple
        // (non-self-crossing) rectangle. Values are SELEXOL's own TRACE, the
        // corpus's one real example (see acad-dwg's entity.rs module doc).
        let quad = |ctor: fn(Point, Point, Point, Point) -> Entity| {
            ctor(
                Point { x: 7.0, y: 30.725 },
                Point { x: 7.0, y: 30.775 },
                Point { x: 2.5, y: 30.725 },
                Point { x: 2.5, y: 30.775 },
            )
        };
        for e in [
            quad(|p1, p2, p3, p4| Entity::Trace { p1, p2, p3, p4 }),
            quad(|p1, p2, p3, p4| Entity::Solid { p1, p2, p3, p4 }),
        ] {
            let prims = flatten_entity(&e, &vp());
            assert_eq!(prims.len(), 1);
            let Prim::Polyline(pts) = &prims[0];
            assert_eq!(pts.len(), 5, "4 corners plus closing back to the first");
            let (first, last) = (pts[0], *pts.last().unwrap());
            assert!((first.x - last.x).abs() < 1e-9 && (first.y - last.y).abs() < 1e-9);
            // p2 (index 1) then p4 (index 2): the "Z" order, not sequential
            // p3. p4 is (2.5, 30.775); a naive p1,p2,p3,p4 walk would visit
            // p3 (2.5, 30.725) here instead.
            assert!((pts[2].y - vp().to_screen(Point { x: 2.5, y: 30.775 }).y).abs() < 1e-9);
        }
    }
}
