//! HATCH clipping, family sweep and dash phase.
use super::*;
use crate::hatch_pattern::{HatchFamily, HatchWork};
use std::borrow::Cow;

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

/// HLP `HATCH name,style` island handling. Each sweep segment's depth is the
/// number of selected loops enclosing it; see `docs/native-hatch-styles.md`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HatchStyle {
    /// Alternate nesting levels (odd depth), the existing parity rule.
    #[default]
    Normal,
    /// Only the area between an outermost loop and its first islands (depth 1).
    Outermost,
    /// Everything inside an outermost loop (depth at least 1).
    Ignore,
}

/// A HATCH pattern name with its island style, parsed from `name[,style]`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HatchSpec {
    pub pattern: String,
    pub style: HatchStyle,
}

impl HatchSpec {
    /// Parse an upper-cased `name[,N|O|I]` reply; the name is not validated.
    pub(crate) fn parse(reply: &str) -> Result<Self, String> {
        let (pattern, style) = match reply.split_once(',') {
            None => (reply, HatchStyle::Normal),
            Some((pattern, style)) => (
                pattern,
                match style.trim() {
                    "N" => HatchStyle::Normal,
                    "O" => HatchStyle::Outermost,
                    "I" => HatchStyle::Ignore,
                    other => return Err(format!("unknown HATCH style: {other} (use N, O or I)")),
                },
            ),
        };
        Ok(Self {
            pattern: pattern.trim().to_owned(),
            style,
        })
    }
}

/// Resolved HATCH line families (built-in, `U` or file) with the island
/// style; the selection prompt carries this until generation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HatchRequest {
    pub families: Cow<'static, [HatchFamily]>,
    pub style: HatchStyle,
}

pub(crate) fn hatch_sweep_deg(start: f64, end: f64) -> f64 {
    let sweep = (end - start).rem_euclid(360.0);
    if sweep == 0.0 {
        360.0
    } else {
        sweep
    }
}

pub(crate) fn hatch_geometry(
    drawing: &Drawing,
    ids: &[usize],
    request: &HatchRequest,
    scale: f64,
    angle_deg: f64,
) -> Result<Vec<Entity>, String> {
    hatch_geometry_with(
        drawing,
        ids,
        request,
        scale,
        angle_deg,
        &mut HatchWork::default(),
    )
}

fn hatch_geometry_with(
    drawing: &Drawing,
    ids: &[usize],
    request: &HatchRequest,
    scale: f64,
    angle_deg: f64,
    work: &mut HatchWork,
) -> Result<Vec<Entity>, String> {
    if !angle_deg.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return Err("HATCH angle must be finite and scale must be positive and finite".into());
    }
    // Normalize before composing definition angles, including huge angles.
    let angle = angle_deg.rem_euclid(360.0);
    let mut lines = Vec::new();
    // The boundary does not depend on the family; build it once, after the
    // first family's dash check so error precedence is unchanged.
    let mut boundary = None;
    for family in request.families.iter() {
        let remaining = MAX_ARRAY_ENTITIES - lines.len();
        // Rotation of both origin and family normal leaves their dot product
        // unchanged. Compute phase in definition coordinates, then scale it.
        let normal = hatch_direction(family.angle + 90.0);
        let phase = (family.origin.x * normal.x + family.origin.y * normal.y) * scale;
        let direction = hatch_direction(family.angle);
        let origin_along = (family.origin.x * direction.x + family.origin.y * direction.y) * scale;
        let sweep = HatchSweep {
            spacing: family.spacing * scale,
            phase,
            angle: (angle + family.angle).rem_euclid(360.0),
            origin_along,
            drift: family.drift * scale,
            dashes: DashPattern::new(&family.dashes, scale)?,
            style: request.style,
        };
        let boundary = match &mut boundary {
            Some(boundary) => boundary,
            empty => empty.insert(hatch_boundary(drawing, ids)?),
        };
        let strokes = hatch_line_geometry(boundary, &sweep, remaining, work)?;
        // Retain definition family order and the existing center-out sweep.
        lines.extend(strokes);
    }
    Ok(lines)
}

struct HatchSweep {
    spacing: f64,
    phase: f64,
    angle: f64,
    origin_along: f64,
    drift: f64,
    dashes: Option<DashPattern>,
    style: HatchStyle,
}

/// Selected boundary edges chained into closed loops, plus circles.
struct HatchBoundary {
    loops: Vec<Vec<HatchEdge>>,
    circles: Vec<(Point, f64)>,
}

fn edge_ends(edge: HatchEdge) -> (Point, Point) {
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
}

fn hatch_boundary(drawing: &Drawing, ids: &[usize]) -> Result<HatchBoundary, String> {
    let selected: BTreeSet<_> = ids.iter().copied().collect();
    let mut edges = Vec::new();
    let mut circles = Vec::new();
    for object in crate::selection::selectable_items(drawing) {
        if !selected.contains(&object.id) {
            continue;
        }
        let Item::Entity(entity) = object.item else {
            return Err("HATCH does not support REPEAT groups as boundaries".into());
        };
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
            _ => return Err("HATCH supports closed LINE/ARC loops and CIRCLE boundaries".into()),
        }
    }
    if edges.is_empty() && circles.is_empty() {
        return Err("HATCH needs at least one selected boundary object".into());
    }

    let close = |a: Point, b: Point| (a.x - b.x).abs() <= 1e-8 && (a.y - b.y).abs() <= 1e-8;
    let ends: Vec<_> = edges.iter().map(|edge| edge_ends(*edge)).collect();
    // Endpoints sorted by x, so chaining only tests edges whose endpoint x is
    // near the current point instead of every edge. NaN never compares close.
    let mut by_x: Vec<(f64, usize)> = ends
        .iter()
        .enumerate()
        .flat_map(|(index, (a, b))| [(a.x, index), (b.x, index)])
        .filter(|(x, _)| !x.is_nan())
        .collect();
    by_x.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut unused = vec![true; edges.len()];
    let mut first_unused = 0;
    let mut loops = Vec::<Vec<HatchEdge>>::new();
    while let Some(first_edge) = (first_unused..edges.len()).find(|index| unused[*index]) {
        first_unused = first_edge;
        unused[first_edge] = false;
        let (start, next) = ends[first_edge];
        let mut loop_edges = vec![edges[first_edge]];
        let mut current = next;
        while !close(current, start) {
            // A window wider than the 1e-8 tolerance; `close` decides exactly.
            let reach = 2e-8 + current.x.abs() * 1e-15;
            let low = by_x.partition_point(|(x, _)| *x < current.x - reach);
            let mut nearby: Vec<usize> = by_x[low..]
                .iter()
                .take_while(|(x, _)| *x <= current.x + reach)
                .map(|(_, index)| *index)
                .filter(|index| unused[*index])
                .collect();
            nearby.sort_unstable();
            nearby.dedup();
            let candidates: Vec<_> = nearby
                .into_iter()
                .filter_map(|index| {
                    let (a, b) = ends[index];
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
    Ok(HatchBoundary { loops, circles })
}

fn hatch_line_geometry(
    boundary: &HatchBoundary,
    sweep: &HatchSweep,
    remaining: usize,
    work: &mut HatchWork,
) -> Result<Vec<Entity>, String> {
    let spacing = sweep.spacing;
    let phase = sweep.phase;
    let HatchBoundary { loops, circles } = boundary;
    if !spacing.is_finite() || spacing <= 0.0 || !phase.is_finite() {
        return Err("HATCH line spacing is outside the supported range".into());
    }
    let direction = hatch_direction(sweep.angle);
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
    // Reject offsets whose integer grid indices cannot advance safely. Saturating
    // float-to-int casts alone can otherwise stall or overflow the sweep loop.
    if !min.is_finite()
        || !max.is_finite()
        || (min - phase) / spacing <= i64::MIN as f64
        || (max - phase) / spacing >= i64::MAX as f64
    {
        return Err("HATCH boundary offsets exceed the supported grid index range".into());
    }
    let center = min + (max - min) / 2.0;
    // The original starts at the row index truncated toward zero (in-tree
    // oracle, docs/native-hatch-user.md). A quotient within 1e-9 of an integer
    // is that integer, so rotation noise cannot move the start by one row.
    let quotient = (center - phase) / spacing;
    let nearest = quotient.round();
    let center_index = if (quotient - nearest).abs() <= 1e-9 * quotient.abs().max(1.0) {
        nearest
    } else {
        quotient.trunc()
    } as i64;
    let first_index = if phase + center_index as f64 * spacing >= max {
        ((max - phase - 1e-10) / spacing).floor() as i64
    } else if phase + center_index as f64 * spacing < min {
        ((min - phase) / spacing).ceil() as i64
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
            let offset = phase + index as f64 * spacing;
            offset >= min - 1e-10 && offset < max - 1e-10
        } {
            if offsets.len() == MAX_ARRAY_ENTITIES {
                return Err("HATCH exceeds the supported sweep count".into());
            }
            offsets.push((index, phase + index as f64 * spacing));
            index = index.checked_add(direction).ok_or_else(|| {
                "HATCH boundary offsets exceed the supported grid index range".to_owned()
            })?;
        }
    }
    // Index edges (loop order) then circles by a conservative range of the
    // offsets at which they can produce a crossing, so a row visits only
    // the items it can cross. Rows run in two monotone passes (up from
    // `first_index`, then down), each swept with an active set.
    struct FlatEdge {
        loop_id: usize,
        edge: HatchEdge,
        line_da: f64,
        line_db: f64,
    }
    let flat_edges: Vec<FlatEdge> = loops
        .iter()
        .enumerate()
        .flat_map(|(loop_id, edges)| {
            edges.iter().map(move |edge| {
                let (line_da, line_db) = match *edge {
                    HatchEdge::Line(a, b) => (project(a, normal), project(b, normal)),
                    HatchEdge::Arc { .. } => (0.0, 0.0),
                };
                FlatEdge {
                    loop_id,
                    edge: *edge,
                    line_da,
                    line_db,
                }
            })
        })
        .collect();
    // `magnitude` is the coordinate size of the item's points. The margin
    // covers the 1e-10 vertex snap and the rounding of projected points,
    // which scales with coordinate magnitude even when the projected offset
    // is near zero (O/I project rounded arc points, not the centre).
    let widen = |low: f64, high: f64, magnitude: f64| {
        let margin = 1e-9 + (low.abs().max(high.abs()) + magnitude) * 1e-12;
        (low - margin, high + margin)
    };
    let disk_range = |center: Point, radius: f64| {
        let center_offset = project(center, normal);
        widen(
            center_offset - radius,
            center_offset + radius,
            center.x.abs() + center.y.abs() + radius,
        )
    };
    let ranges: Vec<(f64, f64)> = flat_edges
        .iter()
        .map(|fe| match fe.edge {
            HatchEdge::Line(a, b) => {
                let (da, db) = (fe.line_da, fe.line_db);
                let magnitude = a.x.abs().max(b.x.abs()) + a.y.abs().max(b.y.abs());
                widen(da.min(db), da.max(db), magnitude)
            }
            // Whole-circle range: the Normal rule tests the full circle with
            // an angular tolerance.
            HatchEdge::Arc { center, radius, .. } => disk_range(center, radius),
        })
        .chain(
            circles
                .iter()
                .map(|(center, radius)| disk_range(*center, *radius)),
        )
        .collect();
    struct PrepCircle {
        center_offset: f64,
        center_along: f64,
        radius: f64,
    }
    let prep_circles: Vec<PrepCircle> = circles
        .iter()
        .map(|(center, radius)| PrepCircle {
            center_offset: project(*center, normal),
            center_along: project(*center, direction),
            radius: *radius,
        })
        .collect();
    let mut active = ActiveItems::new(&ranges);
    let mut row_items = Vec::new();
    let mut lines = Vec::with_capacity(count);
    let mut intersections: Vec<(f64, usize)> = Vec::new();
    let mut normal_values: Vec<f64> = Vec::new();
    let mut parity_buf: Vec<bool> = Vec::new();
    let mut inside_buf: Vec<bool> = Vec::new();
    for (index, offset) in offsets {
        work.charge(1)?;
        active.advance(offset, index >= first_index, &mut row_items);
        work.visit(row_items.len());
        let (edge_items, circle_items) =
            row_items.split_at(row_items.partition_point(|item| *item < flat_edges.len()));
        // Crossings tagged with their loop; circles follow the edge loops.
        // Use the sweep's existing 1e-10 boundary tolerance at vertices too.
        // A rotated endpoint a few ULPs above a grid line must not lose the
        // line's lower-bound crossing.
        let at_offset = |value: f64| {
            if (value - offset).abs() <= 1e-10 {
                offset
            } else {
                value
            }
        };
        intersections.clear();
        for fe in edge_items.iter().map(|item| &flat_edges[*item]) {
            let loop_id = fe.loop_id;
            match fe.edge {
                HatchEdge::Line(a, b) => {
                    let da = at_offset(fe.line_da);
                    let db = at_offset(fe.line_db);
                    if (da <= offset && offset < db) || (db <= offset && offset < da) {
                        let fraction = (offset - da) / (db - da);
                        let crossing = Point {
                            x: a.x + (b.x - a.x) * fraction,
                            y: a.y + (b.y - a.y) * fraction,
                        };
                        intersections.push((project(crossing, direction), loop_id));
                    }
                }
                HatchEdge::Arc {
                    center,
                    radius,
                    start_deg,
                    end_deg,
                } if sweep.style != HatchStyle::Normal => {
                    // Island styles count depth per loop, so arcs follow the
                    // lines' half-open endpoint rule on monotone pieces.
                    let frame = SweepFrame {
                        normal,
                        direction,
                        offset,
                    };
                    arc_monotone_crossings(center, radius, start_deg, end_deg, &frame, at_offset)
                        .for_each(|along| intersections.push((along, loop_id)));
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
                                intersections.push((along, loop_id));
                            }
                        }
                    }
                }
            }
        }
        for item in circle_items {
            let circle = item - flat_edges.len();
            let c = &prep_circles[circle];
            let loop_id = loops.len() + circle;
            let delta = offset - c.center_offset;
            if delta.abs() < c.radius {
                let half_chord = (c.radius * c.radius - delta * delta).sqrt();
                intersections.push((c.center_along - half_chord, loop_id));
                intersections.push((c.center_along + half_chord, loop_id));
            }
        }
        let segments = match sweep.style {
            HatchStyle::Normal => {
                // Retain the original parity rule exactly: crossings from all
                // loops are merged before pairing.
                normal_values.clear();
                normal_values.extend(intersections.iter().map(|(along, _)| *along));
                normal_values.sort_by(f64::total_cmp);
                normal_values.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
                normal_values
                    .chunks_exact(2)
                    .map(|pair| (pair[0], pair[1]))
                    .collect()
            }
            HatchStyle::Outermost => depth_segments(
                &mut intersections,
                |depth| depth == 1,
                &mut parity_buf,
                &mut inside_buf,
            )?,
            HatchStyle::Ignore => depth_segments(
                &mut intersections,
                |depth| depth >= 1,
                &mut parity_buf,
                &mut inside_buf,
            )?,
        };
        // The original orders a continuous row's crossings by x when the row
        // is closer to horizontal and by y otherwise (ties by y), so such
        // rows run in ascending x or y whatever the pattern direction
        // (in-tree oracle, docs/native-hatch-user.md). Dashed rows, which that
        // evidence does not cover, keep the pattern direction.
        let reverse = sweep.dashes.is_none() && {
            let (dx, dy) = (direction.x, direction.y);
            if dx.abs() - dy.abs() > 1e-12 {
                dx < 0.0
            } else {
                dy < 0.0
            }
        };
        let ordered: Vec<_> = if reverse {
            segments.into_iter().rev().map(|(a, b)| (b, a)).collect()
        } else {
            segments
        };
        for pair in ordered {
            let point = |along: f64| Point {
                x: direction.x * along + normal.x * offset,
                y: direction.y * along + normal.y * offset,
            };
            let mut emit = |a: f64, b: f64| {
                if lines.len() == remaining {
                    return Err(format!(
                        "HATCH pattern exceeds the aggregate line limit of {MAX_ARRAY_ENTITIES}"
                    ));
                }
                let start = point(a);
                let end = point(b);
                if ![start.x, start.y, end.x, end.y]
                    .iter()
                    .all(|value| value.is_finite())
                {
                    return Err("HATCH geometry exceeds the finite coordinate range".into());
                }
                lines.push(if a == b {
                    Entity::Point { origin: start }
                } else {
                    Entity::Line { start, end }
                });
                Ok(())
            };
            if let Some(dashes) = &sweep.dashes {
                dashes.for_each_stroke(
                    pair.0,
                    pair.1,
                    sweep.origin_along + index as f64 * sweep.drift,
                    work,
                    &mut emit,
                )?;
            } else {
                emit(pair.0, pair.1)?;
            }
        }
    }
    Ok(lines)
}

/// Boundary items whose offset range `[low, high]` contains the current row,
/// maintained incrementally while rows move monotonically in one direction.
/// Items with a NaN range can never cross a row and are never active.
struct ActiveItems<'a> {
    ranges: &'a [(f64, f64)],
    /// Item indices ascending by low end, and ascending by high end.
    by_low: Vec<usize>,
    by_high: Vec<usize>,
    /// Current pass direction (`true` = rising offsets) and pass cursors.
    rising: Option<bool>,
    enter: usize,
    leave: usize,
    active: BTreeSet<usize>,
}

impl<'a> ActiveItems<'a> {
    fn new(ranges: &'a [(f64, f64)]) -> Self {
        let valid: Vec<usize> = (0..ranges.len())
            .filter(|item| !ranges[*item].0.is_nan() && !ranges[*item].1.is_nan())
            .collect();
        let mut by_low = valid.clone();
        by_low.sort_by(|a, b| ranges[*a].0.total_cmp(&ranges[*b].0).then(a.cmp(b)));
        let mut by_high = valid;
        by_high.sort_by(|a, b| ranges[*a].1.total_cmp(&ranges[*b].1).then(a.cmp(b)));
        Self {
            ranges,
            by_low,
            by_high,
            rising: None,
            enter: 0,
            leave: 0,
            active: BTreeSet::new(),
        }
    }

    /// Move to `offset` and write the active items, in index order, to `out`.
    /// Within one pass, offsets must be monotone in the `rising` direction.
    fn advance(&mut self, offset: f64, rising: bool, out: &mut Vec<usize>) {
        if self.rising != Some(rising) {
            self.rising = Some(rising);
            self.enter = 0;
            self.leave = 0;
            self.active.clear();
        }
        let count = self.by_low.len();
        if rising {
            // Enter by ascending low end; leave once the high end is passed.
            while self.enter < count && self.ranges[self.by_low[self.enter]].0 <= offset {
                self.active.insert(self.by_low[self.enter]);
                self.enter += 1;
            }
            while self.leave < count && self.ranges[self.by_high[self.leave]].1 < offset {
                self.active.remove(&self.by_high[self.leave]);
                self.leave += 1;
            }
        } else {
            while self.enter < count
                && self.ranges[self.by_high[count - 1 - self.enter]].1 >= offset
            {
                self.active.insert(self.by_high[count - 1 - self.enter]);
                self.enter += 1;
            }
            while self.leave < count && self.ranges[self.by_low[count - 1 - self.leave]].0 > offset
            {
                self.active.remove(&self.by_low[count - 1 - self.leave]);
                self.leave += 1;
            }
        }
        out.clear();
        out.extend(self.active.iter().copied());
    }
}

/// Spans along one sweep line whose enclosing-loop depth satisfies `keep`.
/// Every edge counts a vertex on the line only as its lower end (half-open),
/// so each loop has an even crossing count: a pass-through vertex once, a
/// local minimum twice and a local maximum not at all. Crossings within 1e-9
/// toggle together, so touching loops never leave zero-length spans. An odd
/// count (for example near-coincident junctions) fails rather than corrupting
/// the row.
fn depth_segments(
    crossings: &mut [(f64, usize)],
    keep: impl Fn(usize) -> bool,
    parity: &mut Vec<bool>,
    inside: &mut Vec<bool>,
) -> Result<Vec<(f64, f64)>, String> {
    let loop_count = crossings.iter().map(|(_, id)| id + 1).max().unwrap_or(0);
    parity.clear();
    parity.resize(loop_count, false);
    for (_, id) in crossings.iter() {
        parity[*id] = !parity[*id];
    }
    if parity.contains(&true) {
        return Err("HATCH boundary crossings are inconsistent on a sweep line".into());
    }
    crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
    inside.clear();
    inside.resize(loop_count, false);
    let mut depth = 0usize;
    let mut start = None;
    let mut segments = Vec::new();
    let mut index = 0;
    while index < crossings.len() {
        let at = crossings[index].0;
        while index < crossings.len() && crossings[index].0 - at <= 1e-9 {
            let id = crossings[index].1;
            inside[id] = !inside[id];
            if inside[id] {
                depth += 1;
            } else {
                depth -= 1;
            }
            index += 1;
        }
        match (start, keep(depth)) {
            (None, true) => start = Some(at),
            (Some(from), false) => {
                segments.push((from, at));
                start = None;
            }
            _ => {}
        }
    }
    Ok(segments)
}

struct SweepFrame {
    normal: Point,
    direction: Point,
    offset: f64,
}

/// Sweep-line crossings of an arc split at its normal-direction extrema. Each
/// monotone piece crosses at most once, counted with the half-open
/// `[low, high)` endpoint rule used for LINE edges.
fn arc_monotone_crossings(
    center: Point,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
    frame: &SweepFrame,
    snap: impl Fn(f64) -> f64,
) -> impl Iterator<Item = f64> {
    let project = |point: Point, axis: Point| point.x * axis.x + point.y * axis.y;
    let sweep = hatch_sweep_deg(start_deg, end_deg);
    let normal_deg = frame.normal.y.atan2(frame.normal.x).to_degrees();
    let mut cuts = vec![0.0, sweep];
    for extremum in [normal_deg, normal_deg + 180.0] {
        let at = (extremum - start_deg).rem_euclid(360.0);
        if at > 0.0 && at < sweep {
            cuts.push(at);
        }
    }
    cuts.sort_by(f64::total_cmp);
    let point = |relative: f64| {
        let radians = (start_deg + relative).to_radians();
        Point {
            x: center.x + radius * radians.cos(),
            y: center.y + radius * radians.sin(),
        }
    };
    let center_offset = project(center, frame.normal);
    let center_along = project(center, frame.direction);
    let offset = frame.offset;
    let mut crossings = Vec::new();
    for piece in cuts.windows(2) {
        let low = snap(project(point(piece[0]), frame.normal));
        let high = snap(project(point(piece[1]), frame.normal));
        if (low <= offset && offset < high) || (high <= offset && offset < low) {
            let delta = offset - center_offset;
            let half_chord = (radius * radius - delta * delta).max(0.0).sqrt();
            // Pieces counterclockwise of the normal lie on the -direction side.
            let middle = (start_deg + (piece[0] + piece[1]) / 2.0 - normal_deg).rem_euclid(360.0);
            crossings.push(if middle < 180.0 {
                center_along - half_chord
            } else {
                center_along + half_chord
            });
        }
    }
    crossings.into_iter()
}

fn hatch_direction(angle_deg: f64) -> Point {
    match angle_deg.rem_euclid(360.0) {
        0.0 => Point { x: 1.0, y: 0.0 },
        90.0 => Point { x: 0.0, y: 1.0 },
        180.0 => Point { x: -1.0, y: 0.0 },
        270.0 => Point { x: 0.0, y: -1.0 },
        angle => {
            let (y, x) = angle.to_radians().sin_cos();
            Point { x, y }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2,000 tiny squares at y = 0 and one square at y = 999: 8,004 edges and
    /// 99,950 rows. Every row is still charged once, but rows visit only the
    /// edges whose offset range they can cross (8,004 per row before).
    #[test]
    fn rows_visit_only_edges_they_can_cross() {
        let mut items = Vec::new();
        let mut square = |x: f64, y: f64, size: f64| {
            let corners = [
                Point { x, y },
                Point { x: x + size, y },
                Point {
                    x: x + size,
                    y: y + size,
                },
                Point { x, y: y + size },
            ];
            for index in 0..4 {
                items.push(Item::Entity(Entity::Line {
                    start: corners[index],
                    end: corners[(index + 1) % 4],
                }));
            }
        };
        for index in 0..2000 {
            square(index as f64 * 0.01, 0.0, 0.004);
        }
        square(0.0, 999.0, 0.5);
        let drawing = Drawing {
            header: crate::Editor::default().drawing().header.clone(),
            items,
        };
        let ids: Vec<usize> = crate::selection::selectable_items(&drawing)
            .map(|object| object.id)
            .collect();
        assert_eq!(ids.len(), 8004);
        for style in [
            HatchStyle::Normal,
            HatchStyle::Outermost,
            HatchStyle::Ignore,
        ] {
            let request = HatchRequest {
                families: Cow::Owned(vec![HatchFamily {
                    angle: 0.0,
                    origin: Point { x: 0.0, y: 0.0 },
                    spacing: 0.0100001,
                    drift: 0.0,
                    dashes: Cow::Borrowed(&[]),
                }]),
                style,
            };
            let mut work = HatchWork::default();
            let lines = hatch_geometry_with(&drawing, &ids, &request, 1.0, 0.0, &mut work).unwrap();
            assert_eq!(lines.len(), 2000 + 50, "{style:?}");
            assert_eq!(work.used(), 99_950, "{style:?}");
            // Row 0 meets three edges of each tiny square; the top square's
            // rows meet at most three of its edges each.
            assert!(
                work.visits() <= 3 * 2000 + 3 * 51,
                "{style:?}: {}",
                work.visits()
            );
        }
    }

    /// Far from the origin, an arc's projected range around a centre whose
    /// own projection is near zero must still cover the rounded arc points
    /// the O/I crossing test projects (review q1 P2-1: a row 2e-10 below the
    /// apex skipped the arc and failed with inconsistent crossings).
    #[test]
    fn far_arcs_keep_their_apex_rows_in_island_styles() {
        let center = Point {
            x: 21213203.435675148,
            y: 21213203.43579576,
        };
        let radius = 0.001;
        let at = |angle: f64, distance: f64| {
            let radians = f64::to_radians(angle);
            Point {
                x: center.x + distance * radians.cos(),
                y: center.y + distance * radians.sin(),
            }
        };
        let (p135, p315, q) = (at(135.0, radius), at(315.0, radius), at(45.0, 0.002));
        let drawing = Drawing {
            header: crate::Editor::default().drawing().header.clone(),
            items: vec![
                Item::Entity(Entity::Arc {
                    center,
                    radius,
                    start_deg: 135.0,
                    end_deg: 315.0,
                }),
                Item::Entity(Entity::Line {
                    start: p315,
                    end: q,
                }),
                Item::Entity(Entity::Line {
                    start: q,
                    end: p135,
                }),
            ],
        };
        let ids: Vec<usize> = crate::selection::selectable_items(&drawing)
            .map(|object| object.id)
            .collect();
        let hatch = |style| {
            let request = HatchRequest {
                families: Cow::Owned(vec![HatchFamily {
                    angle: 0.0,
                    origin: Point {
                        x: 0.0,
                        y: 0.0010852923479412078,
                    },
                    spacing: radius / 7.0,
                    drift: 0.0,
                    dashes: Cow::Borrowed(&[]),
                }]),
                style,
            };
            hatch_geometry(&drawing, &ids, &request, 1.0, 45.0)
        };
        assert_eq!(hatch(HatchStyle::Normal).unwrap().len(), 14);
        for style in [HatchStyle::Outermost, HatchStyle::Ignore] {
            assert_eq!(hatch(style).unwrap().len(), 14, "{style:?}");
        }
    }
}
