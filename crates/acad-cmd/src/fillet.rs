//! Drawing-scoped radius dialogue and atomic two-line FILLET geometry.
use crate::{
    geometry::{line_points, set_line_points},
    parse::number,
    selection::{selectable_count, selected_item_indexes, selection},
    Editor, Effect, InputState,
};
use acad_model::{Entity, Item, Point};

fn sub(a: Point, b: Point) -> Point {
    Point {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}
fn add(a: Point, b: Point) -> Point {
    Point {
        x: a.x + b.x,
        y: a.y + b.y,
    }
}
fn scale(p: Point, f: f64) -> Point {
    Point {
        x: p.x * f,
        y: p.y * f,
    }
}
fn cross(a: Point, b: Point) -> f64 {
    a.x * b.y - a.y * b.x
}
fn length(p: Point) -> f64 {
    p.x.hypot(p.y)
}
fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
fn unit(p: Point) -> Result<Point, String> {
    let len = length(p);
    if !finite(p) || !len.is_finite() || len == 0.0 {
        return Err("FILLET lines must have nonzero finite lengths".into());
    }
    Ok(Point {
        x: p.x / len,
        y: p.y / len,
    })
}
struct Geometry {
    first: (Point, Point),
    second: (Point, Point),
    arc: Option<Entity>,
}
// Retain ordered endpoint rays for an interior intersection, preserving the
// existing retained positive-radius case. Outside a segment, extend its nearer
// endpoint, keeping the farther endpoint. At an endpoint keep the other end.
fn retained(line: (Point, Point), vertex: Point, along: f64, first: bool) -> bool {
    if along > 0.0 && along < 1.0 {
        first
    } else {
        length(sub(line.1, vertex)) >= length(sub(line.0, vertex))
    }
}
fn replace(line: (Point, Point), keep_end: bool, tangent: Point) -> (Point, Point) {
    if keep_end {
        (tangent, line.1)
    } else {
        (line.0, tangent)
    }
}
fn geometry(
    first: (Point, Point),
    second: (Point, Point),
    radius: f64,
) -> Result<Geometry, String> {
    if !radius.is_finite()
        || radius < 0.0
        || ![first.0, first.1, second.0, second.1]
            .into_iter()
            .all(finite)
    {
        return Err("FILLET radius and coordinates must be finite, radius nonnegative".into());
    }
    let a = unit(sub(first.1, first.0))?;
    let b = unit(sub(second.1, second.0))?;
    let denominator = cross(a, b);
    if denominator.abs() <= 1e-12 {
        return Err("FILLET lines must be nonparallel".into());
    }
    let distance = sub(second.0, first.0);
    let along_a = cross(distance, b) / denominator;
    let along_b = cross(distance, a) / denominator;
    let vertex = add(first.0, scale(a, along_a));
    if !finite(vertex) || !along_a.is_finite() || !along_b.is_finite() {
        return Err("FILLET intersection exceeds finite range".into());
    }
    let first_keep = retained(first, vertex, along_a / length(sub(first.1, first.0)), true);
    let second_keep = retained(
        second,
        vertex,
        along_b / length(sub(second.1, second.0)),
        false,
    );
    if radius == 0.0 {
        return Ok(Geometry {
            first: replace(first, first_keep, vertex),
            second: replace(second, second_keep, vertex),
            arc: None,
        });
    }
    let first_ray = sub(if first_keep { first.1 } else { first.0 }, vertex);
    let second_ray = sub(if second_keep { second.1 } else { second.0 }, vertex);
    let au = unit(first_ray)?;
    let bu = unit(second_ray)?;
    let angle = (au.x * bu.x + au.y * bu.y).clamp(-1.0, 1.0).acos();
    if angle <= 1e-10 || std::f64::consts::PI - angle <= 1e-10 {
        return Err("FILLET cannot round a zero or straight angle".into());
    }
    let tangent_distance = radius / (angle / 2.0).tan();
    if !tangent_distance.is_finite()
        || tangent_distance >= length(first_ray)
        || tangent_distance >= length(second_ray)
    {
        return Err("FILLET radius is too large for the retained line ends".into());
    }
    let at = add(vertex, scale(au, tangent_distance));
    let bt = add(vertex, scale(bu, tangent_distance));
    let bisector = unit(add(au, bu))?;
    let center = add(vertex, scale(bisector, radius / (angle / 2.0).sin()));
    if at == vertex
        || bt == vertex
        || center == vertex
        || tangent_distance <= 0.0
        || ![at, bt, center].into_iter().all(finite)
        || !finite(sub(
            center,
            Point {
                x: radius,
                y: radius,
            },
        ))
        || !finite(add(
            center,
            Point {
                x: radius,
                y: radius,
            },
        ))
    {
        return Err("FILLET arc exceeds finite geometry range".into());
    }
    let degrees = |p: Point| {
        let d = (p.y - center.y)
            .atan2(p.x - center.x)
            .to_degrees()
            .rem_euclid(360.0);
        if d > 360.0 - 1e-10 {
            0.0
        } else {
            d
        }
    };
    let (start_deg, end_deg) = if cross(au, bu) > 0.0 {
        (degrees(bt), degrees(at))
    } else {
        (degrees(at), degrees(bt))
    };
    Ok(Geometry {
        first: replace(first, first_keep, at),
        second: replace(second, second_keep, bt),
        arc: Some(Entity::Arc {
            center,
            radius,
            start_deg,
            end_deg,
        }),
    })
}
impl Editor {
    pub(crate) fn submit_fillet(
        &mut self,
        state: InputState,
        line: &str,
    ) -> Result<Effect, String> {
        match state {
            InputState::FilletSelection if line.eq_ignore_ascii_case("R") => {
                self.state = InputState::FilletRadius
            }
            InputState::FilletSelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                if ids.len() != 2 {
                    return Err("FILLET requires exactly two line entities".into());
                }
                self.fillet(&ids, self.drawing.header.fillet_radius)?;
                self.state = InputState::Command;
            }
            InputState::FilletRadius => {
                let radius = if line.is_empty() {
                    self.drawing.header.fillet_radius
                } else {
                    number(line)?
                };
                if !radius.is_finite() || radius < 0.0 {
                    return Err("fillet radius must be nonnegative".into());
                }
                if self.drawing.header.fillet_radius != radius {
                    self.save_undo();
                    self.drawing.header.fillet_radius = radius;
                }
                self.state = InputState::Command;
            }
            _ => unreachable!("FILLET states only"),
        }
        Ok(Effect::Continue)
    }
    fn fillet(&mut self, ids: &[usize], radius: f64) -> Result<(), String> {
        let selected: Vec<_> = selected_item_indexes(&self.drawing, ids)
            .into_iter()
            .collect();
        if selected.len() != 2 {
            return Err("FILLET requires exactly two line entities".into());
        }
        let get = |position: usize| -> Result<(Point, Point), String> {
            match self.drawing.items.get(position - 1) {
                Some(Item::Entity(entity)) => {
                    line_points(entity).ok_or_else(|| "FILLET only supports LINE entities".into())
                }
                _ => Err("FILLET only supports top-level LINE entities".into()),
            }
        };
        let first = get(selected[0])?;
        let second = get(selected[1])?;
        let result = geometry(first, second, radius)?;
        if result.first != first || result.second != second || result.arc.is_some() {
            self.save_undo();
            for (index, line) in selected.iter().zip([result.first, result.second]) {
                if let Some(Item::Entity(entity)) = self.drawing.items.get_mut(index - 1) {
                    set_line_points(entity, line.0, line.1);
                }
            }
            if let Some(arc) = result.arc {
                self.drawing.items.push(Item::Entity(Entity::OnLayer {
                    layer: self.drawing.header.current_layer,
                    entity: Box::new(arc),
                }));
            }
            self.refresh_after_edit();
        }
        self.status = "Filleted two lines".into();
        Ok(())
    }
}
