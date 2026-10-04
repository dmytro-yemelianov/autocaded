//! CHANGE point/property continuation (docs/native-change.md). Every answer
//! is staged on full replacement records; one commit makes one UNDO step.
//! Prompt order follows the original: entities are visited in reverse
//! drawing order, all INSERTs share one angle prompt asked at the first
//! INSERT visited, and each TEXT asks height, angle and text in turn.
use crate::{
    entity_ops::{apply_change_point, bare, can_change_point, item_anchor, set_insert_angle},
    input_state::InputState,
    parse::layer_index,
    selection::{selectable_count, selected_item_indexes, selection},
    text::{angle_input, relative_point},
    Editor, Effect,
};
use acad_model::{Entity, Item, Point};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    InsertAngle,
    TextHeight(usize),
    TextAngle(usize),
    TextValue(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PendingChange {
    replacements: Vec<(usize, Entity)>,
    sources: Vec<Item>,
    steps: VecDeque<Step>,
    /// The intersection point; `None` after a blank answer keeps locations.
    point: Option<Point>,
    /// New TEXT values in visiting order, by replacement position.
    values: Vec<(usize, String)>,
}

/// The pending CHANGE property prompts.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChangeInput(PendingChange);

impl ChangeInput {
    fn step(&self) -> Step {
        *self.0.steps.front().expect("pending CHANGE has a step")
    }
    pub(crate) fn prompt(&self) -> &'static str {
        match self.step() {
            Step::InsertAngle => "CHANGE: new angle (Enter keeps current)",
            Step::TextHeight(_) => "CHANGE TEXT: new height (Enter keeps current)",
            Step::TextAngle(_) => "CHANGE TEXT: new angle or point (Enter keeps current)",
            Step::TextValue(_) => "CHANGE TEXT: new text (Enter keeps current)",
        }
    }
    pub(crate) fn accepts_point(&self) -> bool {
        matches!(self.step(), Step::InsertAngle | Step::TextAngle(_))
    }
    pub(crate) fn accepts_literal_text(&self) -> bool {
        matches!(self.step(), Step::TextValue(_))
    }
}

fn text_properties(
    entity: &mut Entity,
    origin: Option<Point>,
    height: Option<f64>,
    angle: Option<f64>,
    value: Option<&str>,
) {
    match entity {
        Entity::OnLayer { entity, .. } => text_properties(entity, origin, height, angle, value),
        Entity::Text {
            origin: old_origin,
            height: old_height,
            rotation_deg,
            value: old_value,
        } => {
            if let Some(p) = origin {
                *old_origin = p;
            }
            if let Some(h) = height {
                *old_height = h;
            }
            if let Some(a) = angle {
                *rotation_deg = a;
            }
            if let Some(v) = value {
                *old_value = v.to_owned();
            }
        }
        _ => {}
    }
}

fn anchor(entity: &Entity) -> Option<Point> {
    crate::entity_ops::entity_anchor(entity)
}

impl Editor {
    fn commit_change(&mut self, pending: PendingChange) -> Result<Effect, String> {
        for ((index, _), source) in pending.replacements.iter().zip(&pending.sources) {
            if self.drawing.items.get(*index) != Some(source) {
                return Err("CHANGE source changed while properties were pending".into());
            }
        }
        // A new TEXT value erases the staged record (keeping its old value)
        // and appends the new text, in visiting order, as the original does.
        let mut items = Vec::new();
        for (position, (index, entity)) in pending.replacements.iter().enumerate() {
            let replaced = pending.values.iter().any(|(at, _)| *at == position);
            let item = if replaced {
                Item::Erased(entity.clone())
            } else {
                Item::Entity(entity.clone())
            };
            items.push((*index, item));
        }
        let appended: Vec<Item> = pending
            .values
            .iter()
            .map(|(position, value)| {
                let mut entity = pending.replacements[*position].1.clone();
                text_properties(&mut entity, None, None, None, Some(value));
                Item::Entity(entity)
            })
            .collect();
        if !appended.is_empty()
            || items
                .iter()
                .any(|(index, item)| self.drawing.items[*index] != *item)
        {
            self.save_undo();
            for (index, item) in items {
                self.drawing.items[index] = item;
            }
            self.drawing.items.extend(appended);
            self.refresh_after_edit();
            self.status = "Changed entity properties".into();
        }
        self.state = InputState::Command;
        Ok(Effect::Continue)
    }

    fn continue_change(&mut self, pending: PendingChange) -> Result<Effect, String> {
        if pending.steps.is_empty() {
            self.commit_change(pending)
        } else {
            self.state = InputState::ChangeProperties(ChangeInput(pending));
            Ok(Effect::Continue)
        }
    }

    fn stage_change_point(&self, ids: &[usize], line: &str) -> Result<PendingChange, String> {
        let indexes = selected_item_indexes(&self.drawing, ids);
        let mut replacements = Vec::new();
        let mut sources = Vec::new();
        for position in indexes {
            let index = position - 1;
            let source = &self.drawing.items[index];
            let group = "CHANGE point mode does not support REPEAT groups";
            let Item::Entity(e) = source else {
                return Err(group.into());
            };
            if matches!(bare(e), Entity::Repeat(_)) {
                return Err(group.into());
            }
            // The original leaves other kinds (ARC, POINT, TRACE, SOLID...) unchanged.
            if can_change_point(e) || matches!(bare(e), Entity::Text { .. }) {
                replacements.push((index, e.clone()));
                sources.push(source.clone());
            }
        }
        let point = if line.trim().is_empty() {
            None
        } else {
            let base = sources
                .iter()
                .find_map(item_anchor)
                .or_else(|| {
                    selected_item_indexes(&self.drawing, ids)
                        .into_iter()
                        .find_map(|position| item_anchor(&self.drawing.items[position - 1]))
                })
                .ok_or("CHANGE needs an entity with a point")?;
            Some(relative_point(line, base)?)
        };
        let mut steps = VecDeque::new();
        let mut insert_seen = false;
        for (position, (_, e)) in replacements.iter_mut().enumerate().rev() {
            match bare(e) {
                Entity::Text { .. } => {
                    text_properties(e, point, None, None, None);
                    steps.extend([
                        Step::TextHeight(position),
                        Step::TextAngle(position),
                        Step::TextValue(position),
                    ]);
                }
                Entity::Insert { .. } => {
                    if let Some(point) = point {
                        apply_change_point(e, point);
                    }
                    if !insert_seen {
                        insert_seen = true;
                        steps.push_back(Step::InsertAngle);
                    }
                }
                _ => {
                    if let Some(point) = point {
                        apply_change_point(e, point);
                        if matches!(bare(e), Entity::Circle { radius, .. } if !radius.is_finite()) {
                            return Err("CHANGE radius exceeds the finite range".into());
                        }
                    }
                }
            }
        }
        Ok(PendingChange {
            replacements,
            sources,
            steps,
            point,
            values: Vec::new(),
        })
    }

    pub(crate) fn submit_change(
        &mut self,
        state: InputState,
        line: &str,
        input: &str,
    ) -> Result<Effect, String> {
        match state {
            InputState::ChangeSelection => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.state = InputState::ChangeIntersection(ids);
                Ok(Effect::Continue)
            }
            InputState::ChangeIntersection(ids) if line.eq_ignore_ascii_case("L") => {
                self.state = InputState::ChangeLayer(ids);
                Ok(Effect::Continue)
            }
            InputState::ChangeLayer(ids) => {
                let layer = layer_index(line)?;
                self.change_layer(&ids, layer);
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::ChangeIntersection(ids) => {
                let pending = self.stage_change_point(&ids, line)?;
                if pending.replacements.is_empty() {
                    self.status = "CHANGE: no LINE, CIRCLE, INSERT or TEXT selected".into();
                    self.state = InputState::Command;
                    return Ok(Effect::Continue);
                }
                self.continue_change(pending)
            }
            InputState::ChangeProperties(ChangeInput(mut pending)) => {
                let step = pending.steps.front().copied().expect("pending CHANGE step");
                match step {
                    Step::InsertAngle => {
                        if !line.is_empty() {
                            let anchor = pending.point.or_else(|| {
                                pending.replacements.iter().rev().find_map(|(_, e)| {
                                    matches!(bare(e), Entity::Insert { .. })
                                        .then(|| anchor(e))
                                        .flatten()
                                })
                            });
                            let angle = angle_input(line, anchor.expect("INSERT has an origin"))?;
                            for (_, e) in &mut pending.replacements {
                                set_insert_angle(e, angle);
                            }
                        }
                    }
                    Step::TextHeight(position) => {
                        if !line.is_empty() {
                            let height = crate::parse::number(line)?;
                            if !(height > 0.0 && height.is_finite()) {
                                return Err("CHANGE TEXT height must be positive and finite".into());
                            }
                            text_properties(
                                &mut pending.replacements[position].1,
                                None,
                                Some(height),
                                None,
                                None,
                            );
                        }
                    }
                    Step::TextAngle(position) => {
                        if !line.is_empty() {
                            let origin = anchor(&pending.replacements[position].1)
                                .expect("TEXT has an origin");
                            let angle = angle_input(line, origin)?;
                            text_properties(
                                &mut pending.replacements[position].1,
                                None,
                                None,
                                Some(angle),
                                None,
                            );
                        }
                    }
                    Step::TextValue(position) => {
                        if !input.is_empty() {
                            self.measure_text(self.drawing.items.len(), input)?;
                            pending.values.push((position, input.to_owned()));
                        }
                    }
                }
                pending.steps.pop_front();
                self.continue_change(pending)
            }
            _ => unreachable!("CHANGE routing"),
        }
    }
}
