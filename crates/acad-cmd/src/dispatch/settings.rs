//! Settings prompt handling, extracted without changing command policy.
use super::*;

impl Editor {
    pub(super) fn submit_settings(
        &mut self,
        state: InputState,
        line: &str,
    ) -> Result<Effect, String> {
        match state {
            InputState::SketchIncrement => {
                let increment = number(line)?;
                if increment <= 0.0 || !increment.is_finite() {
                    return Err("SKETCH record increment must be positive".into());
                }
                self.start_sketch(increment);
                Ok(Effect::Continue)
            }
            InputState::Delay => {
                // The original reads a signed 16-bit integer and treats a
                // negative count as no delay; fractions are rejected. Native
                // policy refuses values outside -32768..=32767 rather than
                // wrapping them, and counts milliseconds
                // (docs/native-scripts.md).
                let count = line
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| format!("DELAY requires an integer: {}", line.trim()))?;
                let count = i16::try_from(count)
                    .map_err(|_| "DELAY must be from -32768 to 32767".to_owned())?;
                self.state = InputState::Command;
                Ok(Effect::Delay(count.max(0) as u16))
            }
            InputState::UnitsFormat => {
                let format = if line.is_empty() {
                    self.drawing.header.units.format
                } else {
                    match line.trim() {
                        "1" => UnitFormat::Scientific,
                        "2" => UnitFormat::Decimal,
                        "3" => UnitFormat::Engineering,
                        "4" => UnitFormat::Architectural,
                        _ => return Err("UNITS choice must be 1, 2, 3, or 4".into()),
                    }
                };
                self.state = InputState::UnitsPrecision(format);
                Ok(Effect::Continue)
            }
            InputState::UnitsPrecision(format) => {
                let precision = if line.is_empty() {
                    self.drawing.header.units.precision
                } else {
                    line.trim()
                        .parse::<u16>()
                        .map_err(|_| "UNITS precision must be a whole number")?
                };
                match format {
                    UnitFormat::Architectural
                        if !matches!(precision, 1 | 2 | 4 | 8 | 16 | 32 | 64) =>
                    {
                        return Err("UNITS denominator must be 1, 2, 4, 8, 16, 32, or 64".into());
                    }
                    UnitFormat::Architectural => {}
                    _ if precision > 8 => {
                        return Err("UNITS precision must be from 0 to 8".into());
                    }
                    _ => {}
                }
                let units = Units { format, precision };
                if self.drawing.header.units != units {
                    self.save_undo();
                    self.drawing.header.units = units;
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Base => {
                self.drawing.header.base = point(line)?;
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Axis => {
                match line.to_ascii_uppercase().as_str() {
                    "OFF" | "NO" => self.drawing.header.axis.on = false,
                    "ON" | "YES" => self.drawing.header.axis.on = true,
                    _ => {
                        let uppercase = line.to_ascii_uppercase();
                        let (spacing_text, snap_multiple) = match uppercase.strip_suffix('X') {
                            Some(number) => (number.trim(), true),
                            None => (line, false),
                        };
                        let spacing = number(spacing_text)?
                            * if snap_multiple {
                                self.drawing.header.snap.spacing
                            } else {
                                1.0
                            };
                        if spacing <= 0.0 || !spacing.is_finite() {
                            return Err("axis tick spacing must be positive".into());
                        }
                        self.drawing.header.axis.on = true;
                        self.drawing.header.axis.spacing = spacing;
                    }
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Snap | InputState::Grid => {
                let snap = matches!(&self.state, InputState::Snap);
                let previous = if snap {
                    self.drawing.header.snap
                } else {
                    self.drawing.header.grid
                };
                let mode = if snap {
                    parse_mode(line, previous)?
                } else {
                    parse_grid_mode(line, previous, self.drawing.header.snap.spacing)?
                };
                if snap {
                    self.drawing.header.snap = mode;
                } else {
                    self.drawing.header.grid = mode;
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Ortho | InputState::Fill => {
                let enabled = parse_toggle(line)?;
                if matches!(&self.state, InputState::Ortho) {
                    self.drawing.header.ortho = enabled;
                } else {
                    self.drawing.header.fill = enabled;
                }
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::LimitsMin => {
                self.state = InputState::LimitsMax(point(line)?);
                Ok(Effect::Continue)
            }
            InputState::LimitsMax(min) => {
                let max = point_from(line, min)?;
                if max.x <= min.x || max.y <= min.y {
                    return Err("LIMITS upper-right must exceed lower-left".into());
                }
                self.drawing.header.limits = Extents {
                    xmin: min.x,
                    ymin: min.y,
                    xmax: max.x,
                    ymax: max.y,
                };
                self.state = InputState::Command;
                Ok(Effect::Continue)
            }
            InputState::Layer => self.submit_layer(line),
            InputState::LayerVisibility(on) => self.set_layer_visibility(line, on),
            InputState::LayerColor => self.set_layer_color(line),
            InputState::ColorValue => {
                let color = color_index(line)?;
                let layer = self.drawing.header.current_layer;
                if self.drawing.header.layers.get(&layer) != Some(&color) {
                    self.save_undo();
                    self.drawing.header.layers.insert(layer, color);
                }
                self.state = InputState::ColorTargetLayer;
                Ok(Effect::Continue)
            }
            InputState::ColorTargetLayer => {
                if line.is_empty() {
                    self.state = InputState::Command;
                } else {
                    let layer = layer_index(line)?;
                    if self.drawing.header.current_layer != layer {
                        self.save_undo();
                        self.ensure_layer(layer);
                        self.drawing.header.current_layer = layer;
                    }
                    self.state = InputState::Command;
                }
                Ok(Effect::Continue)
            }
            _ => unreachable!("dispatch routes only settings prompts"),
        }
    }
}

mod layers;
