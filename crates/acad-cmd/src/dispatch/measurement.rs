//! Measurement prompt handling, extracted without changing command policy.
use super::*;

impl Editor {
    pub(super) fn submit_measurement(
        &mut self,
        state: InputState,
        line: &str,
    ) -> Result<Effect, String> {
        match state {
            InputState::DistanceFirstPoint => {
                self.state = InputState::DistanceSecondPoint(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::DistanceSecondPoint(first) => {
                let second = point_from(line, first)?;
                let dx = second.x - first.x;
                let dy = second.y - first.y;
                let distance = dx.hypot(dy);
                self.status = format!(
                    "Distance={}",
                    format_measurement(distance, self.drawing.header.units)
                );
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::IdPoint => {
                let location = point(line)?;
                self.status = format!(
                    "X = {}    Y = {}",
                    format_measurement(location.x, self.drawing.header.units),
                    format_measurement(location.y, self.drawing.header.units)
                );
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::AreaFirstPoint => {
                if line.is_empty() {
                    return self.cancel();
                }
                let trimmed = line.trim();
                if trimmed.eq_ignore_ascii_case("E") || trimmed.eq_ignore_ascii_case("ENTITY") {
                    self.state = InputState::AreaSelection;
                    return Ok(Effect::Continue);
                }
                self.state = InputState::AreaNextPoint(vec![point(line)?]);
                Ok(Effect::Continue)
            }
            InputState::AreaNextPoint(mut points) => {
                if line.is_empty() {
                    if points.len() < 3 {
                        return Err("AREA needs at least three points".into());
                    }
                    let (area, _) = polygon_metrics(&points);
                    self.status = format!("Area = {area:.4}");
                    self.state = InputState::Command;
                } else {
                    let previous = *points.last().expect("AREA has a first point");
                    points.push(point_from(line, previous)?);
                    self.state = InputState::AreaNextPoint(points);
                }
                Ok(Effect::Continue)
            }
            InputState::AreaSelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.measure_area(&ids)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }

            _ => unreachable!("dispatch routes only measurement prompts"),
        }
    }
}
