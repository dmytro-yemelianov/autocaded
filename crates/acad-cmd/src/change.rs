//! CHANGE property continuation stages full replacements before one commit.
use crate::{
    entity_ops::{apply_change_point, bare, can_change_point, item_anchor, set_insert_angle},
    input_state::InputState,
    parse::layer_index,
    selection::{selectable_count, selected_item_indexes, selection},
    text::{angle_input, relative_point},
    Editor, Effect,
};
use acad_model::{Entity, Item, Point};
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PendingChange {
    replacements: Vec<(usize, Entity)>,
    sources: Vec<Item>,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ChangeInput {
    InsertAngle(PendingChange, Point),
    TextAngle(PendingChange, Point),
    TextValue(PendingChange),
}
impl ChangeInput {
    pub(crate) fn prompt(&self) -> &'static str {
        match self {
            Self::InsertAngle(..) => "CHANGE: new angle (Enter keeps current)",
            Self::TextAngle(..) => "CHANGE TEXT: new angle or point (Enter keeps current)",
            Self::TextValue(..) => "CHANGE TEXT: new value (Enter keeps current)",
        }
    }
    pub(crate) fn accepts_point(&self) -> bool {
        matches!(self, Self::InsertAngle(..) | Self::TextAngle(..))
    }
}
fn text_properties(
    entity: &mut Entity,
    origin: Option<Point>,
    angle: Option<f64>,
    value: Option<&str>,
) {
    match entity {
        Entity::OnLayer { entity, .. } => text_properties(entity, origin, angle, value),
        Entity::Text {
            origin: old_origin,
            rotation_deg,
            value: old_value,
            ..
        } => {
            if let Some(p) = origin {
                *old_origin = p;
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
impl Editor {
    fn commit_change(&mut self, pending: PendingChange) -> Result<Effect, String> {
        for ((index, _), source) in pending.replacements.iter().zip(&pending.sources) {
            if self.drawing.items.get(*index) != Some(source) {
                return Err("CHANGE source changed while properties were pending".into());
            }
        }
        if pending
            .replacements
            .iter()
            .any(|(index, e)| self.drawing.items[*index] != Item::Entity(e.clone()))
        {
            self.save_undo();
            for (index, e) in pending.replacements {
                self.drawing.items[index] = Item::Entity(e);
            }
            self.refresh_after_edit();
            self.status = "Changed entity properties".into();
        }
        self.state = InputState::Command;
        Ok(Effect::Continue)
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
                let indexes = selected_item_indexes(&self.drawing, &ids);
                let mut replacements = Vec::new();
                let mut sources = Vec::new();
                let mut text_count = 0;
                let mut insert = false;
                for position in indexes {
                    let index = position - 1;
                    let source = &self.drawing.items[index];
                    let Item::Entity(e) = source else {
                        return Err("CHANGE point mode only supports top-level entities".into());
                    };
                    if matches!(bare(e), Entity::Text { .. }) {
                        text_count += 1;
                    } else if !can_change_point(e) {
                        return Err(
                            "CHANGE point mode supports LINE, CIRCLE, INSERT and one TEXT".into(),
                        );
                    }
                    insert |= matches!(bare(e), Entity::Insert { .. });
                    replacements.push((index, e.clone()));
                    sources.push(source.clone());
                }
                if text_count > 0 && (text_count != 1 || replacements.len() != 1) {
                    return Err("CHANGE TEXT properties require a single TEXT; mixed or multiple TEXT selection is unsupported".into());
                }
                let base = sources
                    .iter()
                    .find_map(item_anchor)
                    .ok_or("CHANGE needs an entity with a point")?;
                let point = relative_point(line, base)?;
                for (_, e) in &mut replacements {
                    if text_count == 1 {
                        text_properties(e, Some(point), None, None);
                    } else {
                        apply_change_point(e, point);
                        if matches!(bare(e),Entity::Circle{radius,..} if !radius.is_finite()) {
                            return Err("CHANGE radius exceeds the finite range".into());
                        }
                    }
                }
                let pending = PendingChange {
                    replacements,
                    sources,
                };
                if text_count == 1 {
                    self.state =
                        InputState::ChangeProperties(ChangeInput::TextAngle(pending, point));
                    Ok(Effect::Continue)
                } else if insert {
                    self.state =
                        InputState::ChangeProperties(ChangeInput::InsertAngle(pending, point));
                    Ok(Effect::Continue)
                } else {
                    self.commit_change(pending)
                }
            }
            InputState::ChangeProperties(ChangeInput::InsertAngle(mut pending, point)) => {
                if !line.is_empty() {
                    let angle = angle_input(line, point)?;
                    for (_, e) in &mut pending.replacements {
                        set_insert_angle(e, angle);
                    }
                }
                self.commit_change(pending)
            }
            InputState::ChangeProperties(ChangeInput::TextAngle(mut pending, point)) => {
                if !line.is_empty() {
                    let angle = angle_input(line, point)?;
                    text_properties(&mut pending.replacements[0].1, None, Some(angle), None);
                }
                self.state = InputState::ChangeProperties(ChangeInput::TextValue(pending));
                Ok(Effect::Continue)
            }
            InputState::ChangeProperties(ChangeInput::TextValue(mut pending)) => {
                if !input.is_empty() {
                    self.measure_text(pending.replacements[0].0, input)?;
                    text_properties(&mut pending.replacements[0].1, None, None, Some(input));
                }
                self.commit_change(pending)
            }
            _ => unreachable!("CHANGE routing"),
        }
    }
}
