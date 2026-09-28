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

/// A block may insert another block; `SELEXOL` nests two deep (`PACKTWR`
/// inserts `HEAD`, `COOLER` inserts `ARROW`). The insert graph comes from the
/// file, not from anything this program controls, so a cycle is possible — a
/// single corrupt block-name field is enough (`A` inserts `B` inserts `A`) —
/// and unbounded recursion on that graph is a stack overflow, not a missing
/// shape or an error message (confirmed by running this module's
/// `a_block_that_inserts_itself_terminates` test against an uncapped version
/// of `expand_insert`: it aborted with a stack overflow, not a hang).
///
/// `ACAD.EXE` itself carries the literal string `NESTED BLOCK` at `DS:0x3740`,
/// among its entity-regeneration errors (`BLOCK ERROR IN EREGEN` at
/// `DS:0x374D`, `** UNEXPECTED END-OF-BLOCK` at `DS:0x3763`, `** Undefined
/// block: %s` at `DS:0x377E`) — `EREGEN` being the regeneration path this
/// function reimplements. That is evidence AutoCAD 1.4 imposed *some* nesting
/// limit and raised a named error rather than recursing without bound, i.e.
/// that a cap is the right shape of answer. It is not evidence for what the
/// limit's value was, or even for which direction it constrained: the string
/// could equally concern a nested block *definition* encountered mid-regen
/// (rejected outright) as a nested block *reference* like the one here (an
/// insert whose body inserts another block). `SELEXOL` shipped as a sample
/// drawing and nests references two deep, which argues at least one level of
/// reference nesting was allowed. Nothing in the corpus nests past depth 2, so
/// no observed behaviour here depends on the exact number.
///
/// 16 is therefore ours, not 1983's — picked generously above the corpus's
/// observed depth of 2, not recovered from the binary. Whoever resolves the
/// open question above should start at `DS:0x3740`.
///
/// When the cap is reached, `expand_insert` returns an empty `Vec` for that
/// branch: the offending sub-tree's geometry is silently dropped rather than
/// reported, because `flatten` returns `Vec<Prim>` and has no error channel
/// to carry a diagnosis back through. That silence is deliberate, not an
/// oversight — the alternative is a `Result`-based `flatten` for a case
/// nothing in the corpus exercises.
const MAX_INSERT_DEPTH: u32 = 16;

#[allow(clippy::too_many_arguments)]
fn expand_insert(
    d: &Drawing,
    origin: Point,
    x_scale: f64,
    y_scale: f64,
    rotation_deg: f64,
    name: &str,
    vp: &Viewport,
    depth_remaining: u32,
) -> Vec<Prim> {
    if depth_remaining == 0 {
        return Vec::new();
    }
    let Some(b) = d.block(name) else {
        return Vec::new();
    };
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let mut out = Vec::new();
    for inner in &b.entities {
        let prims = match inner {
            Entity::Insert {
                origin: o2,
                x_scale: xs2,
                y_scale: ys2,
                rotation_deg: r2,
                name: n2,
            } => expand_insert(d, *o2, *xs2, *ys2, *r2, n2, vp, depth_remaining - 1),
            other => flatten_entity(other, vp),
        };
        for prim in prims {
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
    out
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
                out.extend(expand_insert(
                    d,
                    *origin,
                    *x_scale,
                    *y_scale,
                    *rotation_deg,
                    name,
                    vp,
                    MAX_INSERT_DEPTH,
                ));
            }
            other => out.extend(flatten_entity(other, vp)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use acad_model::{
        header::{DwgView, Header, Mode},
        Block, Entity, Extents, Item, Point,
    };

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

    /// `Header` has no `Default` and `acad-model`'s own `test_header` is private
    /// to its test module. `flatten` reads only `Drawing::items`, so every field
    /// here is a zero that no assertion depends on.
    fn test_header() -> Header {
        let zero = Extents {
            xmin: 0.0,
            xmax: 0.0,
            ymin: 0.0,
            ymax: 0.0,
        };
        Header {
            extents: zero,
            limits: zero,
            base: Point { x: 0.0, y: 0.0 },
            view: DwgView {
                center: Point { x: 0.0, y: 0.0 },
                height: 0.0,
            },
            snap: Mode {
                on: false,
                spacing: 0.0,
            },
            grid: Mode {
                on: false,
                spacing: 0.0,
            },
            ortho: false,
            fill: false,
            text_size: 0.0,
            trace_width: 0.0,
            current_layer: 0,
            layers: Default::default(),
        }
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

    #[test]
    fn an_insert_inside_a_block_body_is_expanded() {
        // SELEXOL's PACKTWR inserts HEAD; COOLER inserts ARROW. Without this,
        // those parts of the drawing are silently missing.
        let d = Drawing {
            header: test_header(),
            items: vec![
                Item::Block(Block {
                    name: "INNER".into(),
                    base: Point { x: 0.0, y: 0.0 },
                    entities: vec![Entity::Line {
                        start: Point { x: 0.0, y: 0.0 },
                        end: Point { x: 1.0, y: 1.0 },
                    }],
                }),
                Item::Block(Block {
                    name: "OUTER".into(),
                    base: Point { x: 0.0, y: 0.0 },
                    entities: vec![Entity::Insert {
                        origin: Point { x: 2.0, y: 2.0 },
                        x_scale: 1.0,
                        y_scale: 1.0,
                        rotation_deg: 0.0,
                        name: "INNER".into(),
                    }],
                }),
                Item::Entity(Entity::Insert {
                    origin: Point { x: 0.0, y: 0.0 },
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotation_deg: 0.0,
                    name: "OUTER".into(),
                }),
            ],
        };
        assert_eq!(
            flatten(&d, &vp()).len(),
            1,
            "the LINE inside INNER, reached through OUTER, must be drawn"
        );
    }

    #[test]
    fn a_block_that_inserts_itself_terminates() {
        // Review Focus 1: the file controls this graph. A corrupt name field is
        // enough to make a cycle, and unbounded recursion is a stack overflow,
        // not an error message.
        let d = Drawing {
            header: test_header(),
            items: vec![
                Item::Block(Block {
                    name: "LOOP".into(),
                    base: Point { x: 0.0, y: 0.0 },
                    entities: vec![Entity::Insert {
                        origin: Point { x: 1.0, y: 1.0 },
                        x_scale: 1.0,
                        y_scale: 1.0,
                        rotation_deg: 0.0,
                        name: "LOOP".into(),
                    }],
                }),
                Item::Entity(Entity::Insert {
                    origin: Point { x: 0.0, y: 0.0 },
                    x_scale: 1.0,
                    y_scale: 1.0,
                    rotation_deg: 0.0,
                    name: "LOOP".into(),
                }),
            ],
        };
        // The assertion is that this returns at all.
        let _ = flatten(&d, &vp());
    }

    #[test]
    fn nested_insert_composes_outer_and_inner_transforms_exactly() {
        // `an_insert_inside_a_block_body_is_expanded` above only pins that
        // something is drawn: every transform it exercises is the identity
        // except one translation, so an implementation that applied only one
        // level's transform, or dropped the outer block's placement
        // entirely, would still produce one polyline landing on the same
        // point. This test gives every parameter — both blocks' `base`,
        // both inserts' scale (x != y, so a transposed axis would show),
        // both inserts' origin, and a non-90-degree rotation at *each*
        // level — a distinct, non-identity value, so a broken composition
        // lands somewhere else.
        //
        // The expected point below is derived from the INSERT transform
        // definition itself (translate by -base, scale, rotate, translate
        // by origin — the same formula `expand_insert` documents itself as
        // implementing), not by running this code and recording its output.
        //
        // INNER's own LINE start point, in INNER's local frame:
        //   p = (4.0, 3.0)
        // INNER block's base point:
        //   base_in = (1.0, 2.0)
        // The INSERT-of-INNER living inside OUTER's body:
        //   origin_in = (10.0, -5.0), scale_in = (2.0, 0.5), rot_in = 30 deg
        //
        // Applying INNER's placement to p:
        //   d1 = p - base_in                = (3.0, 1.0)
        //   s1 = (d1.x*2.0, d1.y*0.5)        = (6.0, 0.5)
        //   sin(30 deg) = 0.5, cos(30 deg) = sqrt(3)/2 = 0.8660254037844387
        //   q  = origin_in + (s1.x*cos30 - s1.y*sin30, s1.x*sin30 + s1.y*cos30)
        //      = (10.0 + (6.0*0.8660254037844387 - 0.5*0.5),
        //         -5.0 + (6.0*0.5 + 0.5*0.8660254037844387))
        //      = (14.946152422706632, -1.566987298107781)
        // `q` is a point in OUTER's local frame — where a broken
        // implementation that stops composing after one level would land.
        //
        // OUTER block's base point:
        //   base_out = (-2.0, 1.0)
        // The top-level INSERT referencing OUTER:
        //   origin_out = (100.0, 50.0), scale_out = (3.0, 1.5), rot_out = 60 deg
        //
        // Applying OUTER's placement to q:
        //   d2 = q - base_out                = (16.946152422706632, -2.5669872981077813)
        //   s2 = (d2.x*3.0, d2.y*1.5)         = (50.838457268119896, -3.850480947161672)
        //   sin(60 deg) = sqrt(3)/2 = 0.8660254037844386, cos(60 deg) = 0.5000000000000001
        //   final = origin_out + (s2.x*cos60 - s2.y*sin60, s2.x*sin60 + s2.y*cos60)
        //         = (128.75384295108992, 92.10215500982062)
        let d = Drawing {
            header: test_header(),
            items: vec![
                Item::Block(Block {
                    name: "INNER".into(),
                    base: Point { x: 1.0, y: 2.0 },
                    entities: vec![Entity::Line {
                        start: Point { x: 4.0, y: 3.0 },
                        end: Point { x: 6.0, y: 7.0 },
                    }],
                }),
                Item::Block(Block {
                    name: "OUTER".into(),
                    base: Point { x: -2.0, y: 1.0 },
                    entities: vec![Entity::Insert {
                        origin: Point { x: 10.0, y: -5.0 },
                        x_scale: 2.0,
                        y_scale: 0.5,
                        rotation_deg: 30.0,
                        name: "INNER".into(),
                    }],
                }),
                Item::Entity(Entity::Insert {
                    origin: Point { x: 100.0, y: 50.0 },
                    x_scale: 3.0,
                    y_scale: 1.5,
                    rotation_deg: 60.0,
                    name: "OUTER".into(),
                }),
            ],
        };
        let prims = flatten(&d, &vp());
        assert_eq!(prims.len(), 1);
        let Prim::Polyline(pts) = &prims[0];
        // Compare in world space, not screen space, so the assertion reads
        // in the drawing's own units and does not depend on the viewport's
        // fit/scale (`to_world` is `to_screen`'s exact inverse, up to f64
        // rounding — pinned separately by `to_world_inverts_to_screen` in
        // viewport.rs).
        let world = vp().to_world(pts[0]);
        let expected = Point {
            x: 128.753_842_951_089_92,
            y: 92.102_155_009_820_62,
        };
        assert!(
            (world.x - expected.x).abs() < 1e-9 && (world.y - expected.y).abs() < 1e-9,
            "composed transform landed at {world:?}, expected {expected:?}"
        );
    }
}
