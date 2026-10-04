//! Retained HLP ARC alternatives; every completed arc passes geometric validation.
use super::*;
use crate::geometry::{arc, circle};

fn validate_center(start: Point, center: Point) -> Result<(), String> {
    circle::validate(center, (start.x - center.x).hypot(start.y - center.y))
        .map(|_| ())
        .map_err(|e| e.replace("CIRCLE", "ARC"))
}

impl Editor {
    pub(super) fn submit_arc(&mut self, state: InputState, line: &str) -> Result<Effect, String> {
        let geometry = match state {
            InputState::ArcStart => {
                self.state = if line.is_empty() {
                    InputState::ArcContinueEnd(
                        self.curve_tangent()
                            .ok_or("no previous LINE/ARC to continue")?,
                    )
                } else if line.eq_ignore_ascii_case("C") {
                    InputState::ArcCenterFirst
                } else {
                    InputState::ArcMiddle(point(line)?)
                };
                return Ok(Effect::Continue);
            }
            InputState::ArcCenterFirst => {
                self.state = InputState::ArcStartAfterCenter(point(line)?);
                return Ok(Effect::Continue);
            }
            InputState::ArcStartAfterCenter(center) => {
                let start = curve_point_from(line, center)?;
                validate_center(start, center)?;
                self.state = InputState::ArcCenterChoice(start, center);
                return Ok(Effect::Continue);
            }
            InputState::ArcMiddle(start) => {
                self.state = if line.eq_ignore_ascii_case("C") {
                    InputState::ArcCenter(start)
                } else if line.eq_ignore_ascii_case("E") {
                    InputState::ArcEndpoint(start)
                } else {
                    let middle = curve_point_from(line, start)?;
                    if start == middle {
                        return Err("ARC needs distinct points".into());
                    }
                    InputState::ArcEnd(start, middle)
                };
                return Ok(Effect::Continue);
            }
            InputState::ArcEnd(start, middle) => {
                arc::three_points(start, middle, curve_point_from(line, middle)?)?
            }
            InputState::ArcCenter(start) => {
                let center = curve_point_from(line, start)?;
                validate_center(start, center)?;
                self.state = InputState::ArcCenterChoice(start, center);
                return Ok(Effect::Continue);
            }
            InputState::ArcCenterChoice(start, center) => {
                if line.eq_ignore_ascii_case("A") || line.eq_ignore_ascii_case("L") {
                    self.state = if line.eq_ignore_ascii_case("A") {
                        InputState::ArcCenterAngle(start, center)
                    } else {
                        InputState::ArcCenterChord(start, center)
                    };
                    return Ok(Effect::Continue);
                }
                arc::center_end(start, center, curve_point_from(line, start)?)?
            }
            InputState::ArcCenterAngle(start, center) => {
                arc::center_angle(start, center, number(line)?)?
            }
            InputState::ArcCenterChord(start, center) => {
                arc::center_chord(start, center, number(line)?)?
            }
            InputState::ArcEndpoint(start) => {
                let end = curve_point_from(line, start)?;
                if start == end {
                    return Err("ARC needs distinct endpoints".into());
                }
                self.state = InputState::ArcEndChoice(start, end);
                return Ok(Effect::Continue);
            }
            InputState::ArcEndChoice(start, end) => {
                self.state = match line.to_ascii_uppercase().as_str() {
                    "R" => InputState::ArcEndRadius(start, end),
                    "A" => InputState::ArcEndAngle(start, end),
                    "D" => InputState::ArcEndDirection(start, end),
                    _ => return Err("ARC expects R, A, or D".into()),
                };
                return Ok(Effect::Continue);
            }
            InputState::ArcEndRadius(start, end) => arc::end_radius(start, end, number(line)?)?,
            InputState::ArcEndAngle(start, end) => arc::end_angle(start, end, number(line)?)?,
            InputState::ArcEndDirection(start, end) => {
                let direction = if line.contains(',') || line.starts_with('@') {
                    let p = curve_point_from(line, start)?;
                    Point {
                        x: p.x - start.x,
                        y: p.y - start.y,
                    }
                } else {
                    let (sin, cos) = crate::parse::sin_cos_degrees(number(line)?);
                    Point { x: cos, y: sin }
                };
                arc::end_direction(start, end, direction)?
            }
            InputState::ArcContinueEnd(tangent) => arc::end_direction(
                tangent.end,
                curve_point_from(line, tangent.end)?,
                tangent.direction,
            )?,
            _ => unreachable!("only ARC prompts route here"),
        };
        self.add(geometry.entity);
        self.remember_curve(Some(geometry.tangent));
        self.state = InputState::Command;
        Ok(Effect::Continue)
    }
}
