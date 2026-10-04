//! Retained HLP CIRCLE dialogue; geometry is committed only after validation.
use super::*;
use crate::geometry::circle::{diameter_endpoints, through_three_points, validate};

fn circle_point_from(line: &str, base: Point) -> Result<Point, String> {
    let p = point_from(line, base)?;
    if !p.x.is_finite() || !p.y.is_finite() {
        return Err("CIRCLE coordinates exceed the finite range".into());
    }
    Ok(p)
}

impl Editor {
    pub(super) fn submit_circle(
        &mut self,
        state: InputState,
        line: &str,
    ) -> Result<Effect, String> {
        let circle = match state {
            InputState::CircleCenter => {
                self.state = match line.to_ascii_uppercase().as_str() {
                    "2P" => InputState::CircleTwoPointFirst,
                    "3P" => InputState::CircleThreePointFirst,
                    _ => InputState::CircleRadius(point(line)?),
                };
                return Ok(Effect::Continue);
            }
            InputState::CircleRadius(center) => {
                if line.eq_ignore_ascii_case("D") {
                    self.state = InputState::CircleDiameter(center);
                    return Ok(Effect::Continue);
                }
                let radius = if line.contains(',') || line.starts_with('@') {
                    let p = circle_point_from(line, center)?;
                    (p.x - center.x).hypot(p.y - center.y)
                } else {
                    number(line)?
                };
                validate(center, radius)?
            }
            InputState::CircleDiameter(center) => validate(center, number(line)? * 0.5)?,
            InputState::CircleTwoPointFirst => {
                self.state = InputState::CircleTwoPointSecond(point(line)?);
                return Ok(Effect::Continue);
            }
            InputState::CircleTwoPointSecond(first) => {
                diameter_endpoints(first, circle_point_from(line, first)?)?
            }
            InputState::CircleThreePointFirst => {
                self.state = InputState::CircleThreePointSecond(point(line)?);
                return Ok(Effect::Continue);
            }
            InputState::CircleThreePointSecond(first) => {
                let second = circle_point_from(line, first)?;
                if first == second {
                    return Err("CIRCLE needs distinct points".into());
                }
                self.state = InputState::CircleThreePointThird(first, second);
                return Ok(Effect::Continue);
            }
            InputState::CircleThreePointThird(first, second) => {
                through_three_points(first, second, circle_point_from(line, second)?)?
            }
            _ => unreachable!("only CIRCLE prompts route here"),
        };
        self.add(Entity::Circle {
            center: circle.0,
            radius: circle.1,
        });
        self.state = InputState::Command;
        Ok(Effect::Continue)
    }
}
