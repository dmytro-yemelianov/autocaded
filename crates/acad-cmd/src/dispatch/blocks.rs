//! Blocks prompt handling, extracted without changing command policy.
use super::*;
use crate::external_insert::InsertTarget;
use crate::star_insert::Placement;

impl Editor {
    pub(super) fn submit_blocks(
        &mut self,
        state: InputState,
        line: &str,
    ) -> Result<Effect, String> {
        match state {
            InputState::InsertName => {
                if line.is_empty() {
                    return Err("block name cannot be empty".into());
                }
                let (explode, requested) = match line.strip_prefix('*') {
                    Some(name) => (true, name),
                    None => (false, line),
                };
                let Some(block) = self
                    .drawing
                    .blocks()
                    .find(|block| block.name.eq_ignore_ascii_case(requested))
                else {
                    return Err(format!("unknown block: {requested}"));
                };
                let name = block.name.clone();
                self.state = InputState::InsertOrigin(InsertTarget::existing(name), explode);
                Ok(Effect::Continue)
            }
            InputState::InsertOrigin(name, true) if line.eq_ignore_ascii_case("S") => {
                self.state = InputState::InsertStarXScale(name);
                Ok(Effect::Continue)
            }
            InputState::InsertStarXScale(name) => {
                let scale = if line.is_empty() { 1.0 } else { number(line)? };
                if scale == 0.0 || !scale.is_finite() {
                    return Err("X scale must be finite and nonzero".into());
                }
                self.state = InputState::InsertStarYScale(name, scale);
                Ok(Effect::Continue)
            }
            InputState::InsertStarYScale(name, x_scale) => {
                let y_scale = if line.is_empty() {
                    x_scale
                } else {
                    number(line)?
                };
                if y_scale == 0.0 || !y_scale.is_finite() {
                    return Err("Y scale must be finite and nonzero".into());
                }
                self.state = InputState::InsertStarRotation(name, x_scale, y_scale);
                Ok(Effect::Continue)
            }
            InputState::InsertStarRotation(mut name, x_scale, y_scale) => {
                let rotation_deg = if line.is_empty() { 0.0 } else { number(line)? };
                if !rotation_deg.is_finite() {
                    return Err("rotation angle must be finite".into());
                }
                let placement = Placement {
                    x_scale,
                    y_scale,
                    rotation_deg,
                };
                match &name.import {
                    Some(import) => Self::check_external_star(import, placement)?,
                    None => self.check_star_block(&name.name, placement)?,
                }
                name.placement = placement;
                self.state = InputState::InsertOrigin(name, true);
                Ok(Effect::Continue)
            }
            InputState::InsertOrigin(name, explode) => {
                let origin = point(line)?;
                if explode {
                    match &name.import {
                        Some(import) => self.commit_external_star(&name, import, origin)?,
                        None => self.explode_block(&name.name, origin, name.placement)?,
                    }
                    self.state = InputState::Command;
                } else {
                    self.state = InputState::InsertXScale(name, origin);
                }
                Ok(Effect::Continue)
            }
            InputState::InsertXScale(name, origin) => {
                if line.contains(',') || line.starts_with('@') {
                    let opposite = point_from(line, origin)?;
                    let x_scale = opposite.x - origin.x;
                    let y_scale = opposite.y - origin.y;
                    if x_scale == 0.0 || y_scale == 0.0 {
                        return Err("INSERT scale box needs nonzero width and height".into());
                    }
                    self.state = InputState::InsertRotation(name, origin, x_scale, y_scale);
                    return Ok(Effect::Continue);
                }
                let scale = if line.is_empty() { 1.0 } else { number(line)? };
                if scale == 0.0 {
                    return Err("X scale must be nonzero".into());
                }
                self.state = InputState::InsertYScale(name, origin, scale);
                Ok(Effect::Continue)
            }
            InputState::InsertYScale(name, origin, x_scale) => {
                let y_scale = if line.is_empty() {
                    x_scale
                } else {
                    number(line)?
                };
                if y_scale == 0.0 {
                    return Err("Y scale must be nonzero".into());
                }
                self.state = InputState::InsertRotation(name, origin, x_scale, y_scale);
                Ok(Effect::Continue)
            }
            InputState::InsertRotation(name, origin, x_scale, y_scale) => {
                let rotation_deg = if line.is_empty() { 0.0 } else { number(line)? };
                let insert = Entity::Insert {
                    origin,
                    x_scale,
                    y_scale,
                    rotation_deg,
                    name: name.name.clone(),
                };
                match &name.import {
                    Some(import) => self.commit_external_block(name.name, import, insert),
                    None => self.add(insert),
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::BlockName => {
                if line.is_empty() || !line.bytes().all(|byte| byte.is_ascii_graphic()) {
                    return Err("block name must contain printable ASCII characters".into());
                }
                if self
                    .drawing
                    .blocks()
                    .any(|block| block.name.eq_ignore_ascii_case(line))
                {
                    return Err(format!("block already exists: {line}"));
                }
                self.state = InputState::BlockBase(line.to_ascii_uppercase());
                Ok(Effect::Continue)
            }
            InputState::BlockBase(name) => {
                self.state = InputState::BlockSelection(name, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::BlockSelection(name, base) => {
                let count = selectable_count(&self.drawing);
                let ids = if line.eq_ignore_ascii_case("LAST") {
                    if count == 0 {
                        return Err("there are no selectable entities".into());
                    }
                    vec![count]
                } else {
                    selection(line, count)?
                };
                self.create_block(name, base, &ids);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::RepeatColumns => {
                let columns = positive_word(line, "columns")?;
                self.state = InputState::RepeatRows(columns);
                Ok(Effect::Continue)
            }
            InputState::RepeatRows(columns) => {
                let rows = positive_word(line, "rows")?;
                let start = self.repeat_start.ok_or("ENDREP without REPEAT")?;
                let anchor = self.drawing.items[start..]
                    .iter()
                    .find_map(|item| match item {
                        Item::Entity(entity) => entity_anchor(entity),
                        _ => None,
                    })
                    .unwrap_or(Point { x: 0.0, y: 0.0 });
                self.state = InputState::RepeatColumnSpacing(columns, rows, anchor);
                Ok(Effect::Continue)
            }
            InputState::RepeatColumnSpacing(columns, rows, previous) => {
                let (spacing, next, kind) = match number(line) {
                    Ok(spacing) => (
                        spacing,
                        Point {
                            x: previous.x + spacing,
                            y: previous.y,
                        },
                        RepeatDistanceInput::Number,
                    ),
                    Err(_) => {
                        let next = point_from(line, previous)?;
                        (0.0, next, RepeatDistanceInput::Point)
                    }
                };
                self.state = InputState::RepeatRowSpacing(columns, rows, spacing, next, kind);
                Ok(Effect::Continue)
            }
            InputState::RepeatRowSpacing(columns, rows, column_spacing, previous, kind) => {
                let (column_spacing, row_spacing) = match kind {
                    RepeatDistanceInput::Number => (column_spacing, number(line)?),
                    RepeatDistanceInput::Point => {
                        let next = point_from(line, previous)?;
                        (next.x - previous.x, next.y - previous.y)
                    }
                };
                let start = self.repeat_start.ok_or("ENDREP without REPEAT")?;
                let slice = &self.drawing.items[start..];
                if slice.iter().any(|i| !matches!(i, Item::Entity(_) | Item::Erased(_))) {
                    return Err("REPEAT can only contain ordinary entities".into());
                }
                self.save_undo();
                let entities = self
                    .drawing
                    .items
                    .drain(start..)
                    .map(|i| match i {
                        Item::Entity(e) => e,
                        Item::Erased(e) => {
                            if e.is_erased() {
                                e
                            } else {
                                Entity::Erased(Box::new(e))
                            }
                        }
                        _ => unreachable!(),
                    })
                    .collect();
                self.drawing.items.push(Item::Repeat(acad_model::Repeat {
                    start_layer: self.repeat_start_layer,
                    end_layer: self.drawing.header.current_layer,
                    entities,
                    columns,
                    rows,
                    column_spacing,
                    row_spacing,
                }));
                self.repeat_start = None;
                self.state = InputState::Command;
                self.refresh_after_edit();
                Ok(Effect::Continue)
            }
            _ => unreachable!("dispatch routes only blocks prompts"),
        }
    }
}
