//! Retained HLP syntax with bounded native layer/visibility policy.
use super::*;

impl Editor {
    pub(in crate::dispatch) fn submit_layer(&mut self, line: &str) -> Result<Effect, String> {
        if line.is_empty() {
            self.state = InputState::Command;
            return Ok(Effect::Continue);
        }
        let (option, arguments) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match option.to_ascii_uppercase().as_str() {
            "ON" | "OFF" => {
                let on = option.eq_ignore_ascii_case("ON");
                self.state = InputState::LayerVisibility(on);
                if arguments.trim().is_empty() {
                    Ok(Effect::Continue)
                } else {
                    self.set_layer_visibility(arguments.trim(), on)
                }
            }
            "COLOR" => {
                self.state = InputState::LayerColor;
                if arguments.trim().is_empty() {
                    Ok(Effect::Continue)
                } else {
                    self.set_layer_color(arguments.trim())
                }
            }
            "?" if arguments.trim().is_empty() => {
                let h = &self.drawing.header;
                let mut rows = vec!["Layers: index  color  visibility  current".to_owned()];
                for (&layer, &color) in &h.layers {
                    rows.push(format!(
                        "{layer}  {color}  {}{}",
                        if h.layer_is_visible(layer) {
                            "ON"
                        } else {
                            "OFF"
                        },
                        if h.current_layer == layer {
                            "  current"
                        } else {
                            ""
                        }
                    ));
                }
                self.state = InputState::Command;
                self.status = "Layer report".into();
                Ok(Effect::Report(rows.join("\n")))
            }
            _ => {
                let layer = layer_index(line)?;
                if self.drawing.header.current_layer != layer
                    || !self.drawing.header.layers.contains_key(&layer)
                {
                    self.save_undo();
                    self.ensure_layer(layer);
                    self.drawing.header.current_layer = layer;
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
        }
    }

    pub(super) fn set_layer_visibility(&mut self, line: &str, on: bool) -> Result<Effect, String> {
        if line.is_empty() {
            self.state = InputState::Command;
            return Ok(Effect::Continue);
        }
        let layers = line
            .split(',')
            .map(|text| layer_index(text.trim()))
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        for layer in &layers {
            if !self.drawing.header.layers.contains_key(layer) {
                return Err(format!(
                    "layer {layer} is undefined; select it with LAYER first"
                ));
            }
        }
        if layers
            .iter()
            .any(|layer| self.drawing.header.layer_is_visible(*layer) != on)
        {
            self.save_undo();
            for layer in layers {
                if on {
                    self.drawing.header.off_layers.remove(&layer);
                } else {
                    self.drawing.header.off_layers.insert(layer);
                }
            }
        }
        self.state = InputState::Command;
        Ok(Effect::Continue)
    }

    pub(super) fn set_layer_color(&mut self, line: &str) -> Result<Effect, String> {
        let color = color_index(line)?;
        let layer = self.drawing.header.current_layer;
        if self.drawing.header.layers.get(&layer) != Some(&color) {
            self.save_undo();
            self.drawing.header.layers.insert(layer, color);
        }
        self.state = InputState::Command;
        Ok(Effect::Continue)
    }
}
