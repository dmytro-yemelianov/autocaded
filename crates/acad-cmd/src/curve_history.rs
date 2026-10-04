//! Session-local continuation, validated against its still-live source record.
use crate::Editor;
use acad_model::{Item, Point};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tangent {
    pub(crate) end: Point,
    pub(crate) direction: Point,
}
#[derive(Debug, Clone)]
pub(crate) struct CurveHistory {
    index: usize,
    source: Item,
    tangent: Tangent,
}
impl Editor {
    pub(crate) fn curve_tangent(&self) -> Option<Tangent> {
        let h = self.last_curve.as_ref()?;
        (self.drawing.items.get(h.index) == Some(&h.source)).then_some(h.tangent)
    }
    pub(crate) fn remember_curve(&mut self, tangent: Option<Tangent>) {
        self.last_curve = tangent.map(|tangent| CurveHistory {
            index: self.drawing.items.len() - 1,
            source: self.drawing.items.last().unwrap().clone(),
            tangent,
        });
    }
}
pub(crate) fn line_tangent(start: Point, end: Point) -> Option<Tangent> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx.hypot(dy);
    (length.is_finite() && length > 0.0).then_some(Tangent {
        end,
        direction: Point {
            x: dx / length,
            y: dy / length,
        },
    })
}
