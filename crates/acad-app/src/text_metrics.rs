//! Runtime whole-string SHP ink, normalized by actual cap height. Font context
//! uses the same bounded LOAD walker as rendering, including hidden owners.
use acad_cmd::{TextMetricProvider, TextMetrics};
use acad_model::Drawing;
#[derive(Debug)]
pub(crate) struct RuntimeTextMetrics(pub(crate) acad_render::Libraries);
impl TextMetricProvider for RuntimeTextMetrics {
    fn measure(
        &self,
        drawing: &Drawing,
        before_item: usize,
        value: &str,
    ) -> Result<TextMetrics, String> {
        let name = acad_render::text_font_at(drawing, before_item, &self.0)?;
        let library = self
            .0
            .get(&name)
            .ok_or_else(|| format!("TEXT: missing SHP font {name}"))?;
        let cap = library
            .cap_height
            .ok_or_else(|| format!("TEXT: {name} is not a font"))?;
        let glyph = library
            .text(value)
            .map_err(|e| format!("TEXT font {name}: {e}"))?;
        if glyph.advance.y != 0.0 || !glyph.advance.x.is_finite() {
            return Err("TEXT metrics require horizontal finite font advance".into());
        }
        let mut ink: Option<(f64, f64)> = None;
        for point in glyph.strokes.iter().flatten() {
            if !point.x.is_finite() || !point.y.is_finite() {
                return Err("TEXT font contains nonfinite ink".into());
            }
            ink = Some(ink.map_or((point.x, point.x), |(min, max)| {
                (min.min(point.x), max.max(point.x))
            }));
        }
        Ok(TextMetrics {
            ink_x: ink.map(|(min, max)| (min / cap, max / cap)),
        })
    }
}
