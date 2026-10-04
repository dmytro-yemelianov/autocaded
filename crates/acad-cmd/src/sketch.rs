//! Native mouse SKETCH (docs/native-sketch.md). Pointer samples become
//! temporary segments; only R/X move them into the drawing as LINEs.
use crate::{input_state::InputState, Editor, Effect};
use acad_model::{Entity, Point};

/// Upper bound on temporary (unrecorded) segments held by one SKETCH.
pub const MAX_SKETCH_SEGMENTS: usize = 10_000;

pub(crate) const SKETCH_PROMPT: &str = "Sketch.  Pen eXit Quit Record Erase Connect .";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SketchMode {
    /// Ordinary pen-up/pen-down sketching.
    Draw,
    /// `C`: waiting for the pointer to reach the last end point.
    Connect,
    /// `E`: choosing the first temporary line to erase.
    Erase,
}

/// Read-only SKETCH state for frames and transports.
#[derive(Debug, Clone, PartialEq)]
pub struct SketchPreview {
    pub temporary: Vec<[Point; 2]>,
    /// In erase mode, the index of the first temporary line P would erase.
    pub erase_from: Option<usize>,
    /// Pen-down tail that a pen-up, R or X would record now: the leg to
    /// the constrained pointer and, with ORTHO, the second leg of the L to
    /// the snapped pointer. Empty with the pen up.
    pub rubber: Vec<[Point; 2]>,
    pub pen_down: bool,
    pub mode: SketchMode,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sketch {
    increment: f64,
    pen_down: bool,
    mode: SketchMode,
    last: Option<Point>,
    pointer: Option<Point>,
    temporary: Vec<[Point; 2]>,
}

impl Sketch {
    pub(crate) fn new(increment: f64) -> Self {
        Self {
            increment,
            pen_down: false,
            mode: SketchMode::Draw,
            last: None,
            pointer: None,
            temporary: Vec::new(),
        }
    }

    /// ORTHO anchors on the last vertex only while the pen is down.
    pub(crate) fn ortho_origin(&self) -> Option<Point> {
        self.pen_down.then_some(self.last).flatten()
    }

    /// Append a segment, merging an exactly collinear same-direction
    /// continuation. Returns false when the bound refuses it.
    fn push(&mut self, start: Point, end: Point) -> bool {
        if start == end || merge(&mut self.temporary, start, end) {
            return true;
        }
        if self.temporary.len() >= MAX_SKETCH_SEGMENTS {
            return false;
        }
        self.temporary.push([start, end]);
        true
    }

    /// "Select end of delete": the first temporary line with an end point
    /// nearest `point`. Erasing removes it and every later line, so the
    /// path is cut back through the selected vertex. Earliest wins ties.
    fn erase_index(&self, point: Point) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (index, [a, b]) in self.temporary.iter().enumerate() {
            let distance = distance(point, *a).min(distance(point, *b));
            if best.is_none_or(|(_, nearest)| distance < nearest) {
                best = Some((index, distance));
            }
        }
        best.map(|(index, _)| index)
    }
}

/// Extend the last segment by an exactly collinear same-direction
/// continuation that starts at its end point (native policy beyond the
/// observed axis-aligned runs; it also joins a new stroke that starts there).
fn merge(segments: &mut [[Point; 2]], start: Point, end: Point) -> bool {
    let Some(previous) = segments.last_mut() else {
        return false;
    };
    let a = Point {
        x: previous[1].x - previous[0].x,
        y: previous[1].y - previous[0].y,
    };
    let b = Point {
        x: end.x - start.x,
        y: end.y - start.y,
    };
    if previous[1] == start && a.x * b.y - a.y * b.x == 0.0 && a.x * b.x + a.y * b.y > 0.0 {
        previous[1] = end;
        return true;
    }
    false
}

fn chebyshev(a: Point, b: Point) -> f64 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

fn distance(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

const FULL: &str = "Sketch buffer full (10000 temporary lines): Record (R), eXit (X) or Quit (Q).";

impl Editor {
    pub(crate) fn start_sketch(&mut self, increment: f64) {
        self.state = InputState::Sketch(Box::new(Sketch::new(increment)));
    }

    /// Whether SKETCH is accepting pointer motion and control keys.
    pub fn sketch_active(&self) -> bool {
        matches!(self.state, InputState::Sketch(_))
    }

    pub fn sketch_preview(&self) -> Option<SketchPreview> {
        let InputState::Sketch(sketch) = &self.state else {
            return None;
        };
        let pointer = sketch
            .pointer
            .and_then(|raw| self.constrain_mouse_point(raw).ok());
        Some(SketchPreview {
            temporary: sketch.temporary.clone(),
            erase_from: match (sketch.mode, pointer) {
                (SketchMode::Erase, Some(point)) => sketch.erase_index(point),
                _ => None,
            },
            rubber: match (sketch.last, self.pen_tail()) {
                (Some(last), Ok(Some((corner, point)))) => [[last, corner], [corner, point]]
                    .into_iter()
                    .filter(|[a, b]| a != b)
                    .collect(),
                _ => Vec::new(),
            },
            pen_down: sketch.pen_down,
            mode: sketch.mode,
        })
    }

    /// One pointer-motion sample. Outside SKETCH, motion changes nothing.
    pub fn pointer_moved(&mut self, raw: Point) -> Result<Effect, String> {
        if !self.sketch_active() {
            return Ok(Effect::Continue);
        }
        let point = self.constrain_mouse_point(raw)?;
        let InputState::Sketch(sketch) = &mut self.state else {
            unreachable!("checked above");
        };
        sketch.pointer = Some(raw);
        match (sketch.mode, sketch.last) {
            (SketchMode::Draw, Some(last))
                if sketch.pen_down && chebyshev(point, last) >= sketch.increment =>
            {
                if sketch.push(last, point) {
                    sketch.last = Some(point);
                } else {
                    self.status = FULL.into();
                }
            }
            (SketchMode::Connect, Some(last)) if chebyshev(point, last) <= sketch.increment => {
                sketch.mode = SketchMode::Draw;
                sketch.pen_down = true;
                self.status.clear();
            }
            _ => {}
        }
        Ok(Effect::Continue)
    }

    /// A click toggles the pen at the clicked point.
    pub(crate) fn sketch_click(&mut self, point: Point) -> Result<Effect, String> {
        self.pointer_moved(point)?;
        self.submit("P")
    }

    fn sketch_mut(&mut self) -> &mut Sketch {
        match &mut self.state {
            InputState::Sketch(sketch) => sketch,
            _ => unreachable!("SKETCH control outside SKETCH"),
        }
    }

    /// Constrained pointer, required for pen and erase operations.
    fn sketch_pointer(&self) -> Result<Point, String> {
        let InputState::Sketch(sketch) = &self.state else {
            unreachable!("SKETCH control outside SKETCH");
        };
        let raw = sketch
            .pointer
            .ok_or("SKETCH: move the pointer into the drawing first")?;
        self.constrain_mouse_point(raw)
    }

    /// Record the pen-down tail to the pointer, then raise the pen. With
    /// ORTHO the tail is an L, as observed (O13): to the ORTHO point, then on
    /// to the snapped pointer, both legs axis-aligned.
    fn raise_pen(&mut self) -> Result<(), String> {
        let Some((corner, point)) = self.pen_tail()? else {
            return Ok(());
        };
        let sketch = self.sketch_mut();
        if let Some(last) = sketch.last {
            let mut full = false;
            for (start, end) in [(last, corner), (corner, point)] {
                if sketch.last == Some(start) && sketch.push(start, end) {
                    sketch.last = Some(end);
                } else if sketch.last == Some(start) {
                    full = true;
                }
            }
            if full {
                self.status = FULL.into();
            }
        }
        self.sketch_mut().pen_down = false;
        Ok(())
    }

    /// The pen-down tail as (ORTHO corner, snapped pointer). Without ORTHO
    /// both are the same point. `None` with the pen up.
    fn pen_tail(&self) -> Result<Option<(Point, Point)>, String> {
        let InputState::Sketch(sketch) = &self.state else {
            return Ok(None);
        };
        let Some(raw) = sketch.pointer.filter(|_| sketch.pen_down) else {
            return Ok(None);
        };
        Ok(Some((
            self.constrain_mouse_point(raw)?,
            self.snap_mouse_point(raw)?,
        )))
    }

    /// Commit temporary lines as one undoable batch. With the pen down the
    /// same L-shaped tail as a pen-up is recorded (O13), and sketching
    /// continues from the snapped pointer. The tail joins the batch even when
    /// the temporary buffer is full, so R/X never drop it; the bound limits
    /// only unrecorded lines.
    fn record_sketch(&mut self) -> Result<usize, String> {
        let tail = self.pen_tail()?;
        let sketch = self.sketch_mut();
        let mut segments = std::mem::take(&mut sketch.temporary);
        if let (Some((corner, point)), Some(last)) = (tail, sketch.last) {
            for [start, end] in [[last, corner], [corner, point]] {
                if start != end && !merge(&mut segments, start, end) {
                    segments.push([start, end]);
                }
            }
            sketch.last = Some(point);
        }
        if !segments.is_empty() {
            self.save_undo();
            let layer = self.drawing.header.current_layer;
            for [start, end] in &segments {
                self.drawing
                    .items
                    .push(acad_model::Item::Entity(Entity::OnLayer {
                        layer,
                        entity: Box::new(Entity::Line {
                            start: *start,
                            end: *end,
                        }),
                    }));
            }
            self.refresh_after_edit();
        }
        Ok(segments.len())
    }

    pub(crate) fn submit_sketch(&mut self, line: &str) -> Result<Effect, String> {
        let key = line.to_ascii_uppercase();
        if !matches!(key.as_str(), "" | "P" | "X" | "Q" | "R" | "E" | "C" | ".") {
            return Err("SKETCH: expected P, X, Q, R, E, C, . or Return".into());
        }
        let mut prefix = String::new();
        match (self.sketch_mut().mode, key.as_str()) {
            (SketchMode::Erase, "P") => {
                let point = self.sketch_pointer()?;
                let sketch = self.sketch_mut();
                if let Some(index) = sketch.erase_index(point) {
                    sketch.last = Some(sketch.temporary[index][0]);
                    sketch.temporary.truncate(index);
                }
                sketch.mode = SketchMode::Draw;
                return Ok(Effect::Continue);
            }
            (SketchMode::Erase, "E") => {
                self.sketch_mut().mode = SketchMode::Draw;
                self.status = "Erase aborted.".into();
                return Ok(Effect::Continue);
            }
            (SketchMode::Connect, "C" | "P") => {
                self.sketch_mut().mode = SketchMode::Draw;
                self.status = "Connect aborted.".into();
                return Ok(Effect::Continue);
            }
            (SketchMode::Erase, _) => {
                self.sketch_mut().mode = SketchMode::Draw;
                prefix = "Erase aborted.  ".into();
            }
            (SketchMode::Connect, _) => {
                self.sketch_mut().mode = SketchMode::Draw;
                prefix = "Connect aborted.  ".into();
            }
            (SketchMode::Draw, _) => {}
        }
        let message = match key.as_str() {
            "P" if self.sketch_mut().pen_down => {
                self.raise_pen()?;
                None
            }
            "P" => {
                let point = self.sketch_pointer()?;
                let sketch = self.sketch_mut();
                sketch.pen_down = true;
                sketch.last = Some(point);
                None
            }
            "R" => Some(format!("{} lines recorded.", self.record_sketch()?)),
            "" | "X" => {
                let count = self.record_sketch()?;
                self.state = InputState::Command;
                Some(format!("{count} lines recorded."))
            }
            "Q" => {
                self.state = InputState::Command;
                None
            }
            "E" => {
                if self.sketch_mut().temporary.is_empty() {
                    Some("No temporary lines to erase.".into())
                } else {
                    self.raise_pen()?;
                    self.sketch_mut().mode = SketchMode::Erase;
                    Some("Erase:  Select end of delete.".into())
                }
            }
            "C" => {
                let sketch = self.sketch_mut();
                if sketch.pen_down {
                    Some("Connect command meaningless when pen down.  Connect aborted.".into())
                } else if sketch.last.is_none() {
                    Some("No last point known.  Connect aborted.".into())
                } else {
                    sketch.mode = SketchMode::Connect;
                    self.status = "Connect:  Move to endpoint of line.".into();
                    if let Some(raw) = self.sketch_mut().pointer {
                        self.pointer_moved(raw)?;
                    }
                    return Ok(Effect::Continue);
                }
            }
            "." => {
                let sketch = self.sketch_mut();
                if sketch.pen_down {
                    None
                } else if let Some(last) = sketch.last {
                    let point = self.sketch_pointer()?;
                    let sketch = self.sketch_mut();
                    if sketch.push(last, point) {
                        sketch.last = Some(point);
                        None
                    } else {
                        Some(FULL.into())
                    }
                } else {
                    Some("No last point known.".into())
                }
            }
            _ => unreachable!("validated above"),
        };
        if let Some(message) = message {
            self.status = format!("{prefix}{message}");
        } else if !prefix.is_empty() {
            self.status = prefix.trim_end().into();
        }
        Ok(Effect::Continue)
    }
}
