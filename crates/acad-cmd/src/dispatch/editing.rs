//! Editing prompt handling, extracted without changing command policy.
use super::*;

impl Editor {
    pub(super) fn submit_editing(
        &mut self,
        state: InputState,
        line: &str,
    ) -> Result<Effect, String> {
        match state {
            InputState::EditSelection(command) => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                if command == EditCommand::Erase {
                    self.erase(&ids);
                    self.state = InputState::Command;
                    return Ok(Effect::Continue);
                }
                debug_assert!(matches!(command, EditCommand::Rotate | EditCommand::Scale));
                self.state = InputState::EditBase(command, ids);
                Ok(Effect::Continue)
            }
            InputState::Displacement(command) => {
                self.state = InputState::SecondPoint(command, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::SecondPoint(command, first) => {
                let delta = if line.is_empty() {
                    first
                } else {
                    let second = point_from(line, first)?;
                    Point {
                        x: second.x - first.x,
                        y: second.y - first.y,
                    }
                };
                self.state = InputState::DisplacedSelection(command, delta);
                Ok(Effect::Continue)
            }
            InputState::DisplacedSelection(command, delta) => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.transform(
                    &ids,
                    Transform::Translate(delta),
                    command == EditCommand::Copy,
                )?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::EditBase(command, ids) => {
                self.state = InputState::EditValue(command, ids, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::EditValue(command, ids, base) => {
                let transform = match command {
                    EditCommand::Move | EditCommand::Copy => {
                        unreachable!("MOVE and COPY use displacement prompts")
                    }
                    EditCommand::Rotate => Transform::Rotate {
                        base,
                        degrees: number(line)?,
                    },
                    EditCommand::Scale => {
                        let factor = number(line)?;
                        if factor <= 0.0 {
                            return Err("scale factor must be positive".into());
                        }
                        Transform::Scale { base, factor }
                    }
                    EditCommand::Erase => unreachable!("ERASE has no transform prompts"),
                };
                self.transform(&ids, transform, command == EditCommand::Copy)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ArraySelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.state = InputState::ArrayMode(ids);
                Ok(Effect::Continue)
            }
            InputState::ArrayMode(ids) => {
                if line.eq_ignore_ascii_case("R") || line.eq_ignore_ascii_case("RECTANGULAR") {
                    self.state = InputState::ArrayRows(ids);
                } else if line.eq_ignore_ascii_case("C") || line.eq_ignore_ascii_case("CIRCULAR") {
                    self.state = InputState::ArrayCircularCenter(ids);
                } else {
                    return Err("array mode must be rectangular (R) or circular (C)".into());
                }
                Ok(Effect::Continue)
            }
            InputState::ArrayCircularCenter(ids) => {
                self.state = InputState::ArrayCircularAngle(ids, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::ArrayCircularAngle(ids, center) => {
                let angle = number(line)?;
                if angle == 0.0 || angle.abs() > 360.0 {
                    return Err("angle between items must be nonzero and at most 360".into());
                }
                self.state = InputState::ArrayCircularItems(ids, center, angle);
                Ok(Effect::Continue)
            }
            InputState::ArrayCircularItems(ids, center, angle) => {
                let count = circular_item_count(line, angle, ids.len())?;
                if !(angle * count.saturating_sub(1) as f64).is_finite() {
                    return Err("circular array angle is too large".into());
                }
                if self.single_insert_selected(&ids) {
                    self.state = InputState::ArrayCircularRotate(ids, center, angle, count);
                } else {
                    self.circular_array(&ids, center, angle, count, false)?;
                    self.state = InputState::Command;
                }
                Ok(Effect::Continue)
            }
            InputState::ArrayCircularRotate(ids, center, angle, count) => {
                let rotate = match line.to_ascii_uppercase().as_str() {
                    "Y" | "YES" => true,
                    "" | "N" | "NO" => false,
                    _ => return Err("answer Y or N".into()),
                };
                self.circular_array(&ids, center, angle, count, rotate)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ArrayRows(ids) => {
                let rows = positive_count(line, "row count")?;
                self.state = InputState::ArrayColumns(ids, rows);
                Ok(Effect::Continue)
            }
            InputState::ArrayColumns(ids, rows) => {
                let columns = positive_count(line, "column count")?;
                let total = rows
                    .checked_mul(columns)
                    .ok_or_else(|| "array dimensions are too large".to_owned())?;
                if total.saturating_mul(ids.len()) > MAX_ARRAY_ENTITIES {
                    return Err(format!(
                        "array exceeds the {MAX_ARRAY_ENTITIES}-entity implementation limit"
                    ));
                }
                self.state = InputState::ArrayRowSpacing(ids, rows, columns);
                Ok(Effect::Continue)
            }
            InputState::ArrayRowSpacing(ids, rows, columns) => {
                let row_spacing = match number(line) {
                    Ok(spacing) => ArraySpacingInput::Number(spacing),
                    Err(_) => {
                        let selected = selected_item_indexes(&self.drawing, &ids);
                        let anchor = self
                            .drawing
                            .items
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| selected.contains(&(index + 1)))
                            .find_map(|(_, item)| item_anchor(item))
                            .ok_or("ARRAY needs one or more ordinary entities")?;
                        ArraySpacingInput::Point(point_from(line, anchor)?)
                    }
                };
                self.state = InputState::ArrayColumnSpacing(ids, rows, columns, row_spacing);
                Ok(Effect::Continue)
            }
            InputState::ArrayColumnSpacing(ids, rows, columns, row_input) => {
                let (row_spacing, column_spacing) = match row_input {
                    ArraySpacingInput::Number(row_spacing) => (row_spacing, number(line)?),
                    ArraySpacingInput::Point(first) => {
                        let second = point_from(line, first)?;
                        (second.y - first.y, second.x - first.x)
                    }
                };
                self.array(&ids, rows, columns, row_spacing, column_spacing);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::BreakSelection => {
                // BREAK takes one object, so a typed comma pair is a pick
                // point, never an ID list. Selector results arrive as IDs
                // through `break_select_ids`, never as re-parsed text.
                if let Ok(pick) = point(line) {
                    self.break_pick(pick, self.typed_pick_aperture())?
                        .ok_or("No object found")?;
                    return Ok(Effect::Continue);
                }
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.break_select_ids(ids)
            }
            InputState::BreakFirstPoint(ids) => {
                self.state = InputState::BreakSecondPoint(ids, point(line)?, false);
                Ok(Effect::Continue)
            }
            InputState::BreakSecondPoint(ids, _, true) if line.eq_ignore_ascii_case("F") => {
                self.state = InputState::BreakFirstPoint(ids);
                Ok(Effect::Continue)
            }
            InputState::BreakSecondPoint(ids, first, _) => {
                self.break_entity(&ids, first, point_from(line, first)?)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }

            _ => unreachable!("dispatch routes only editing prompts"),
        }
    }
}

impl Editor {
    /// Entity-number route of BREAK: exactly one object, then explicit points.
    pub(crate) fn break_select_ids(&mut self, ids: Vec<usize>) -> Result<Effect, String> {
        if ids.len() != 1 {
            return Err("BREAK requires exactly one entity".into());
        }
        self.state = InputState::BreakFirstPoint(ids);
        Ok(Effect::Continue)
    }
}

/// Item count for the circular ARRAY count prompt: a positive integer is the
/// count; `v <= 0` covers `-v` degrees, endpoint inclusive except for the
/// exact full circle (0 or 360), with half-up rounding (in-tree measured).
fn circular_item_count(line: &str, angle: f64, selected: usize) -> Result<usize, String> {
    let limit = || format!("array exceeds the {MAX_ARRAY_ENTITIES}-entity implementation limit");
    let value = number(line)?;
    let count = if value > 0.0 {
        positive_count(line, "array item count")?
    } else {
        let fill = -value;
        let steps = if fill == 0.0 || fill == 360.0 {
            (360.0 / angle.abs()).round()
        } else {
            (fill / angle.abs()).round() + 1.0
        };
        if steps.is_nan() || steps > MAX_ARRAY_ENTITIES as f64 {
            return Err(limit());
        }
        (steps as usize).max(1)
    };
    if count.saturating_mul(selected) > MAX_ARRAY_ENTITIES {
        return Err(limit());
    }
    Ok(count)
}
