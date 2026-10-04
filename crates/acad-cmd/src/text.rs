//! Baked ink-edge layout and session-only TEXT continuation. No disk alignment
//! metadata or original line-spacing parity is implied.
use crate::{
    input_state::InputState,
    parse::{number, point_from, sin_cos_degrees},
    Editor, Effect,
};
use acad_model::{Drawing, Entity, Item, Point};
use std::sync::Arc;
mod context;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub ink_x: Option<(f64, f64)>,
}
/// Measure the complete string at the font context immediately before a stored
/// top-level item, or `items.len()` for append. Return cap-height-normalized ink.
pub trait TextMetricProvider: std::fmt::Debug + Send + Sync {
    fn measure(
        &self,
        drawing: &Drawing,
        before_item: usize,
        value: &str,
    ) -> Result<TextMetrics, String>;
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Alignment {
    Left,
    Center,
    Right,
    InkLeft,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Layout {
    pub anchor: Point,
    pub alignment: Alignment,
    pub height: f64,
    pub angle: f64,
    pub span: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TextInput {
    Start,
    Anchor(Alignment),
    AlignedFirst,
    AlignedSecond(Point),
    Height(Point, Alignment),
    HeightSecond(Point, Alignment, Point),
    Angle(Point, Alignment, f64),
    Value(Layout),
}
impl TextInput {
    pub(crate) fn prompt(&self) -> &'static str {
        match self {
            Self::Start => "TEXT: start point (or A/C/R)",
            Self::Anchor(Alignment::Center) => "TEXT: center point",
            Self::Anchor(_) => "TEXT: right endpoint",
            Self::AlignedFirst => "TEXT: first endpoint",
            Self::AlignedSecond(_) => "TEXT: second endpoint",
            Self::Height(..) => "TEXT: height",
            Self::HeightSecond(..) => "TEXT: height second point",
            Self::Angle(..) => "TEXT: rotation angle",
            Self::Value(_) => "TEXT: value",
        }
    }
    pub(crate) fn accepts_point(&self) -> bool {
        !matches!(self, Self::Value(_))
    }
}
#[derive(Debug, Clone)]
pub(crate) struct TextHistory {
    layout: Layout,
    index: usize,
    source: Item,
}
pub(crate) fn finite_point(p: Point) -> Result<Point, String> {
    if p.x.is_finite() && p.y.is_finite() {
        Ok(p)
    } else {
        Err("TEXT/CHANGE coordinates exceed the finite range".into())
    }
}
pub(crate) fn relative_point(line: &str, base: Point) -> Result<Point, String> {
    finite_point(point_from(line, base)?)
}
pub(crate) fn angle_input(line: &str, anchor: Point) -> Result<f64, String> {
    if line.contains(',') || line.starts_with('@') {
        let end = relative_point(line, anchor)?;
        let dx = end.x - anchor.x;
        let dy = end.y - anchor.y;
        if !dx.is_finite() || !dy.is_finite() || dx.hypot(dy) == 0.0 || !dx.hypot(dy).is_finite() {
            return Err("angle point must be finite and distinct from its anchor".into());
        }
        Ok(dy.atan2(dx).to_degrees())
    } else {
        number(line)
    }
}
impl Editor {
    pub fn register_text_metrics(&mut self, provider: Arc<dyn TextMetricProvider>) {
        self.text_metrics = Some(provider);
    }
    pub(crate) fn measure_text(&self, before: usize, value: &str) -> Result<TextMetrics, String> {
        if value.chars().any(char::is_control) {
            return Err(
                "metric-dependent TEXT supports a single line without control characters".into(),
            );
        }
        let metrics = if let Some(provider) = &self.text_metrics {
            provider.measure(&self.drawing, before, value)?
        } else {
            // Standalone command clients have only the verified compiled TXT
            // ASCII metric table. Any LOAD context needs an explicit provider.
            context::startup_txt_context(&self.drawing, before)?;
            let mut pen = 0.0;
            let mut ink: Option<(f64, f64)> = None;
            for ch in value.chars() {
                let index = (ch as u32)
                    .checked_sub(32)
                    .filter(|&i| i < 95)
                    .ok_or("default TXT metrics require defined printable ASCII")?
                    as usize;
                let (advance, bounds) = crate::txt_metrics::TXT_METRICS[index]
                    .ok_or("glyph is missing from default TXT")?;
                if let Some((min, max)) = bounds {
                    let min = pen + min;
                    let max = pen + max;
                    ink = Some(ink.map_or((min, max), |(old_min, old_max)| {
                        (old_min.min(min), old_max.max(max))
                    }));
                }
                pen += advance;
            }
            TextMetrics {
                ink_x: ink.map(|(min, max)| {
                    (
                        min / crate::txt_metrics::TXT_CAP_HEIGHT,
                        max / crate::txt_metrics::TXT_CAP_HEIGHT,
                    )
                }),
            }
        };
        if metrics
            .ink_x
            .is_some_and(|(min, max)| !min.is_finite() || !max.is_finite() || max < min)
        {
            return Err("invalid font ink metrics".into());
        }
        Ok(metrics)
    }
    pub(crate) fn repeat_text(&mut self) -> Result<Effect, String> {
        let Some(history) = self
            .last_text
            .clone()
            .filter(|h| self.drawing.items.get(h.index) == Some(&h.source))
        else {
            return Err("no still-live TEXT continuation in this session".into());
        };
        let mut layout = history.layout;
        let (sin, cos) = sin_cos_degrees(layout.angle);
        layout.anchor = finite_point(Point {
            x: layout.anchor.x + 1.5 * layout.height * sin,
            y: layout.anchor.y - 1.5 * layout.height * cos,
        })?;
        layout.span = None;
        self.state = InputState::Text(TextInput::Value(layout));
        Ok(Effect::Continue)
    }
    pub(crate) fn submit_text(
        &mut self,
        state: TextInput,
        line: &str,
        input: &str,
    ) -> Result<Effect, String> {
        let next = match state {
            TextInput::Start => match line.to_ascii_uppercase().as_str() {
                "A" => TextInput::AlignedFirst,
                "C" => TextInput::Anchor(Alignment::Center),
                "R" => TextInput::Anchor(Alignment::Right),
                _ => TextInput::Height(
                    relative_point(line, Point { x: 0.0, y: 0.0 })?,
                    Alignment::Left,
                ),
            },
            TextInput::Anchor(a) => {
                TextInput::Height(relative_point(line, Point { x: 0.0, y: 0.0 })?, a)
            }
            TextInput::AlignedFirst => {
                TextInput::AlignedSecond(relative_point(line, Point { x: 0.0, y: 0.0 })?)
            }
            TextInput::AlignedSecond(first) => {
                let second = relative_point(line, first)?;
                let length = (second.x - first.x).hypot(second.y - first.y);
                if length <= 0.0 || !length.is_finite() {
                    return Err("aligned endpoints must be finite and distinct".into());
                }
                TextInput::Value(Layout {
                    anchor: first,
                    alignment: Alignment::InkLeft,
                    height: 1.0,
                    angle: angle_input(line, first)?,
                    span: Some(length),
                })
            }
            TextInput::Height(anchor, a) => {
                if line.contains(',') || line.starts_with('@') {
                    TextInput::HeightSecond(anchor, a, relative_point(line, anchor)?)
                } else {
                    let height = number(line)?;
                    if height <= 0.0 {
                        return Err("text height must be positive".into());
                    }
                    TextInput::Angle(anchor, a, height)
                }
            }
            TextInput::HeightSecond(anchor, a, first) => {
                let second = relative_point(line, first)?;
                let height = (second.x - first.x).hypot(second.y - first.y);
                if height <= 0.0 || !height.is_finite() {
                    return Err("height points must be finite and distinct".into());
                }
                TextInput::Angle(anchor, a, height)
            }
            TextInput::Angle(anchor, a, height) => TextInput::Value(Layout {
                anchor,
                alignment: a,
                height,
                angle: angle_input(line, anchor)?,
                span: None,
            }),
            TextInput::Value(mut layout) => {
                if input.is_empty() {
                    return Err("text value cannot be empty".into());
                }
                let offset = if layout.alignment == Alignment::Left {
                    0.0
                } else {
                    let (min, max) = self
                        .measure_text(self.drawing.items.len(), input)?
                        .ink_x
                        .ok_or("aligned/center/right TEXT needs visible ink")?;
                    if let Some(span) = layout.span {
                        let width = max - min;
                        if width <= 0.0 {
                            return Err("aligned TEXT needs nonzero ink width".into());
                        }
                        layout.height = span / width;
                    }
                    match layout.alignment {
                        Alignment::Center => (min + max) / 2.0,
                        Alignment::Right => max,
                        _ => min,
                    }
                };
                if layout.height <= 0.0 || !layout.height.is_finite() {
                    return Err("derived text height exceeds the finite range".into());
                }
                let (sin, cos) = sin_cos_degrees(layout.angle);
                let origin = finite_point(Point {
                    x: layout.anchor.x - offset * layout.height * cos,
                    y: layout.anchor.y - offset * layout.height * sin,
                })?;
                self.add(Entity::Text {
                    origin,
                    height: layout.height,
                    rotation_deg: layout.angle,
                    value: input.to_owned(),
                });
                let index = self.drawing.items.len() - 1;
                self.last_text = Some(TextHistory {
                    layout,
                    index,
                    source: self.drawing.items[index].clone(),
                });
                self.state = InputState::Command;
                return Ok(Effect::Continue);
            }
        };
        self.state = InputState::Text(next);
        Ok(Effect::Continue)
    }
}
