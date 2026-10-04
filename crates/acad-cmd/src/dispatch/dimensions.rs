//! Dimensions prompt handling, extracted without changing command policy.
use super::*;

impl Editor {
    pub(super) fn submit_dimensions(
        &mut self,
        state: InputState,
        line: &str,
    ) -> Result<Effect, String> {
        match state {
            InputState::DimFirstExtension => {
                self.state = match line.to_ascii_uppercase().as_str() {
                    "A" => InputState::DimArrowSize,
                    "T" => InputState::DimInsideHorizontalText {
                        default_horizontal: self.dim_style.inside_horizontal_text,
                    },
                    "B" | "C" => {
                        self.state = InputState::Command;
                        let previous = self.last_dimension.ok_or_else(|| {
                            "DIM B/C needs a completed dimension in this editing session".to_owned()
                        })?;
                        InputState::DimSecondExtension(
                            previous.followup(
                                line.eq_ignore_ascii_case("B"),
                                self.dim_style.arrow_size,
                            ),
                        )
                    }
                    _ => InputState::DimIntersection(point(line)?),
                };
                Ok(Effect::Continue)
            }
            InputState::DimArrowSize => {
                // Exit even on failure. The positive decimal domain is a Rust policy;
                // native scalar acceptance was observed only for the retained samples.
                self.state = InputState::Command;
                let arrow_size = line
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .ok_or_else(|| {
                        "DIM arrow size must be a finite positive decimal number".to_owned()
                    })?;
                self.dim_style.arrow_size = arrow_size;
                Ok(Effect::Continue)
            }
            InputState::DimInsideHorizontalText { default_horizontal } => {
                let pending_inside_horizontal = dim_horizontal_answer(line, default_horizontal)?;
                self.state = InputState::DimOutsideHorizontalText {
                    pending_inside_horizontal,
                    default_horizontal: self.dim_style.outside_horizontal_text,
                };
                Ok(Effect::Continue)
            }
            InputState::DimOutsideHorizontalText {
                pending_inside_horizontal,
                default_horizontal,
            } => {
                let outside_horizontal = dim_horizontal_answer(line, default_horizontal)?;
                // Stage both questions until completion. Retry and cancellation are
                // local policies, not claims about unobserved native lifecycle paths.
                self.dim_style.inside_horizontal_text = pending_inside_horizontal;
                self.dim_style.outside_horizontal_text = outside_horizontal;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::DimIntersection(first) => {
                self.state =
                    InputState::DimSecondExtension(DimInput::new(first, point_from(line, first)?)?);
                Ok(Effect::Continue)
            }
            InputState::DimSecondExtension(input) => {
                self.state = InputState::DimText(input, point_from(line, input.first)?);
                Ok(Effect::Continue)
            }
            InputState::DimText(input, second) => {
                let text = if line.trim().is_empty() {
                    None
                } else {
                    Some(line.trim())
                };
                let dimension = dimension_geometry(
                    input,
                    second,
                    text,
                    self.dim_style,
                    self.drawing.header.units,
                )?;
                self.save_undo();
                self.last_dimension = Some(dimension.history);
                self.drawing.header.dim_arrow = Some(self.dim_style.arrow_size);
                for entity in dimension.entities {
                    self.drawing.items.push(Item::Entity(Entity::OnLayer {
                        layer: self.drawing.header.current_layer,
                        entity: Box::new(entity),
                    }));
                }
                self.state = InputState::Command;
                self.refresh_after_edit();
                Ok(Effect::Continue)
            }

            _ => unreachable!("dispatch routes only dimensions prompts"),
        }
    }
}
