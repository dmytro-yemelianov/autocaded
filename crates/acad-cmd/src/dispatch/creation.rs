//! Creation prompt handling, extracted without changing command policy.
use super::*;
mod arc;
mod circle;

fn curve_point_from(line: &str, base: Point) -> Result<Point, String> {
    let p = point_from(line, base)?;
    if !p.x.is_finite() || !p.y.is_finite() {
        return Err("curve coordinates exceed the finite range".into());
    }
    Ok(p)
}

fn solid_point_from(line: &str, base: Point) -> Result<Point, String> {
    let p = point_from(line, base)?;
    if !p.x.is_finite() || !p.y.is_finite() {
        return Err("SOLID coordinates exceed the finite range".into());
    }
    Ok(p)
}

impl Editor {
    pub(super) fn submit_creation(
        &mut self,
        state: InputState,
        line: &str,
        input: &str,
    ) -> Result<Effect, String> {
        match state {
            InputState::LineStart => {
                let p = if line.is_empty() {
                    let Some(tangent) = self.curve_tangent() else {
                        return self.cancel();
                    };
                    tangent.end
                } else {
                    point(line)?
                };
                self.state = InputState::LineNext {
                    first: p,
                    previous: p,
                };
                Ok(Effect::Continue)
            }
            InputState::LineNext { first, previous } => {
                if line.is_empty() {
                    self.state = InputState::Command;
                    return Ok(Effect::Continue);
                }
                if line.eq_ignore_ascii_case("C") {
                    // Reuse the exact first vertex, including typed points
                    // between SNAP grid positions. Already-closed paths do
                    // not acquire a zero-length closing segment or undo step.
                    if previous != first {
                        self.add(Entity::Line {
                            start: previous,
                            end: first,
                        });
                        self.remember_curve(crate::curve_history::line_tangent(previous, first));
                    }
                    self.state = InputState::Command;
                    return Ok(Effect::Continue);
                }
                let next = curve_point_from(line, previous)?;
                self.add(Entity::Line {
                    start: previous,
                    end: next,
                });
                self.remember_curve(crate::curve_history::line_tangent(previous, next));
                self.state = InputState::LineNext {
                    first,
                    previous: next,
                };
                Ok(Effect::Continue)
            }
            state @ (InputState::CircleCenter
            | InputState::CircleRadius(_)
            | InputState::CircleDiameter(_)
            | InputState::CircleTwoPointFirst
            | InputState::CircleTwoPointSecond(_)
            | InputState::CircleThreePointFirst
            | InputState::CircleThreePointSecond(_)
            | InputState::CircleThreePointThird(_, _)) => self.submit_circle(state, line),
            InputState::Point => {
                let origin = point(line)?;
                self.add(Entity::Point { origin });
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            state @ (InputState::ArcStart
            | InputState::ArcMiddle(..)
            | InputState::ArcEnd(..)
            | InputState::ArcCenterFirst
            | InputState::ArcStartAfterCenter(..)
            | InputState::ArcCenter(..)
            | InputState::ArcCenterChoice(..)
            | InputState::ArcCenterAngle(..)
            | InputState::ArcCenterChord(..)
            | InputState::ArcEndpoint(..)
            | InputState::ArcEndChoice(..)
            | InputState::ArcEndRadius(..)
            | InputState::ArcEndAngle(..)
            | InputState::ArcEndDirection(..)
            | InputState::ArcContinueEnd(..)) => self.submit_arc(state, line),
            InputState::LoadLibrary => {
                if line.is_empty() {
                    return Err("shape library name cannot be empty".into());
                }
                let library = normalize_library_name(line);
                if !self.shape_libraries.contains_key(&library) {
                    return Err(format!("shape library is not available: {line}"));
                }
                self.add(Entity::Load {
                    name: line.to_owned(),
                });
                self.active_shape_library = Some(library);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ShapeName => {
                let Some(library) = self.active_shape_library.as_ref() else {
                    return Err("SHAPE requires a library loaded with LOAD".into());
                };
                let Some(number) = self
                    .shape_libraries
                    .get(library)
                    .and_then(|shapes| shapes.get(&line.to_ascii_uppercase()))
                    .copied()
                else {
                    return Err(format!("unknown shape name: {line}"));
                };
                self.state = InputState::ShapeOrigin(number);
                Ok(Effect::Continue)
            }
            InputState::ShapeOrigin(shape_id) => {
                self.state = InputState::ShapeHeight(shape_id, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::ShapeHeight(shape_id, origin) => {
                let height = number(line)?;
                if height <= 0.0 {
                    return Err("shape height must be positive".into());
                }
                self.state = InputState::ShapeRotation(shape_id, origin, height);
                Ok(Effect::Continue)
            }
            InputState::ShapeRotation(shape_id, origin, height) => {
                let rotation_deg = number(line)?;
                self.add(Entity::Shape {
                    origin,
                    height,
                    rotation_deg,
                    number: shape_id,
                });
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Text(state) => self.submit_text(state, line, input),
            InputState::SolidFirstPoint => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::SolidSecondPoint(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::SolidSecondPoint(p1) => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::SolidThirdPoint(p1, solid_point_from(line, p1)?);
                Ok(Effect::Continue)
            }
            InputState::SolidThirdPoint(p1, p2) => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::SolidFourthPoint(p1, p2, solid_point_from(line, p2)?);
                Ok(Effect::Continue)
            }
            InputState::SolidFourthPoint(p1, p2, p3) => {
                let p4 = if line.is_empty() {
                    p3
                } else {
                    solid_point_from(line, p3)?
                };
                self.add(Entity::Solid { p1, p2, p3, p4 });
                self.state = InputState::SolidThirdPoint(p3, p4);
                Ok(Effect::Continue)
            }
            InputState::TraceWidth => {
                let width = if line.is_empty() {
                    self.drawing.header.trace_width
                } else {
                    number(line)?
                };
                if width <= 0.0 {
                    return Err("TRACE width must be positive".into());
                }
                if width != self.drawing.header.trace_width {
                    self.save_undo();
                    self.drawing.header.trace_width = width;
                }
                self.state = InputState::TraceStart(width);
                Ok(Effect::Continue)
            }
            InputState::TraceStart(width) => {
                if line.is_empty() {
                    return self.cancel();
                }
                self.state = InputState::TraceNext(width, vec![point(line)?]);
                Ok(Effect::Continue)
            }
            InputState::TraceNext(width, mut points) => {
                if line.is_empty() {
                    if points.len() >= 2 {
                        for entity in trace_quads(&points, width)? {
                            self.add(entity);
                        }
                    }
                    self.state = InputState::Command;
                } else {
                    let previous = points.last().expect("TRACE has a first point");
                    let next = point_from(line, *previous)?;
                    if next == *previous {
                        return Err("TRACE needs distinct consecutive points".into());
                    }
                    points.push(next);
                    self.state = InputState::TraceNext(width, points);
                }
                Ok(Effect::Continue)
            }

            _ => unreachable!("dispatch routes only creation prompts"),
        }
    }
}
