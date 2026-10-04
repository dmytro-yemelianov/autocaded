//! Navigation input and finite view fitting. All retains its measured device
//! policy; centered extents/window/lower-left forms use the active viewport.
use crate::{
    dispatch::{fit_box_to_device, zoom_all_bounds},
    parse::{number, point_from},
    selection::visible_bounds::drawing_bounds,
    Editor, Effect, InputState,
};
use acad_model::{DwgView, Extents, Point};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ViewInput {
    Zoom,
    Corner { lower_left: bool },
    Height { corner: Point, lower_left: bool },
    WindowFirst,
    WindowSecond(Point),
    PanFirst,
    PanSecond { first: Point, relative: bool },
}
impl ViewInput {
    pub(crate) fn prompt(&self) -> &'static str {
        match self {
            Self::Zoom => "ZOOM: number, numberX, A, E, W, C, L or P",
            Self::Corner { lower_left: false } => "ZOOM CENTER: x,y",
            Self::Corner { lower_left: true } => "ZOOM LOWER LEFT: x,y",
            Self::Height {
                lower_left: false, ..
            } => "ZOOM CENTER: view height",
            Self::Height {
                lower_left: true, ..
            } => "ZOOM LOWER LEFT: view height",
            Self::WindowFirst => "ZOOM WINDOW: lower-left",
            Self::WindowSecond(_) => "ZOOM WINDOW: upper-right",
            Self::PanFirst => "PAN: from - to x,y or @displacement",
            Self::PanSecond { .. } => "PAN: second point or Return for displacement",
        }
    }
    pub(crate) fn accepts_point(&self) -> bool {
        matches!(
            self,
            Self::Corner { .. }
                | Self::WindowFirst
                | Self::WindowSecond(_)
                | Self::PanFirst
                | Self::PanSecond { .. }
        )
    }
}
fn finite_point(point: Point) -> Result<Point, String> {
    if point.x.is_finite() && point.y.is_finite() {
        Ok(point)
    } else {
        Err("view coordinates exceed finite range".into())
    }
}
fn validate(view: DwgView, aspect: f64, pixel_height: u32) -> Result<DwgView, String> {
    view.validate_canvas(aspect, pixel_height)
        .map_err(str::to_owned)?;
    Ok(view)
}
fn centered_fit(bounds: Extents, aspect: f64) -> DwgView {
    DwgView {
        center: Point {
            x: bounds.xmin / 2.0 + bounds.xmax / 2.0,
            y: bounds.ymin / 2.0 + bounds.ymax / 2.0,
        },
        height: bounds.height().max(bounds.width() / aspect).max(
            if bounds.width() == 0.0 && bounds.height() == 0.0 {
                1.0
            } else {
                0.0
            },
        ),
    }
}
impl Editor {
    /// Native navigation geometry for W/E/L. Standalone clients default to
    /// the retained device aspect; applications pass their drawing canvas.
    pub fn set_viewport_size(&mut self, width: u32, height: u32) -> Result<(), String> {
        if width == 0 || height == 0 {
            return Err("viewport dimensions must be positive".into());
        }
        self.view_aspect = f64::from(width) / f64::from(height);
        self.view_pixel_height = height;
        Ok(())
    }
    fn all_view(&self) -> Result<DwgView, String> {
        // No visible geometry keeps the evidenced empty EXTENTS-at-origin
        // contribution, without importing hidden geometry's stored extents.
        let extents = drawing_bounds(&self.drawing)?.unwrap_or(Extents {
            xmin: 0.0,
            ymin: 0.0,
            xmax: 0.0,
            ymax: 0.0,
        });
        let bounds = zoom_all_bounds(self.drawing.header.limits, extents);
        if ![bounds.xmin, bounds.ymin, bounds.xmax, bounds.ymax]
            .iter()
            .all(|v| v.is_finite())
            || bounds.is_degenerate()
        {
            return Err("LIMITS and visible EXTENTS have no positive finite area".into());
        }
        validate(
            fit_box_to_device(bounds),
            self.view_aspect,
            self.view_pixel_height,
        )
    }
    fn commit_view(&mut self, view: DwgView) -> Result<Effect, String> {
        let view = validate(view, self.view_aspect, self.view_pixel_height)?;
        self.set_view(view.center, view.height);
        self.state = InputState::Command;
        Ok(Effect::Continue)
    }
    pub(crate) fn submit_view(&mut self, input: ViewInput, line: &str) -> Result<Effect, String> {
        match input {
            ViewInput::Zoom => match line.to_ascii_uppercase().as_str() {
                "P" | "PREVIOUS" => {
                    if let Some(previous) = self.previous_view {
                        return self.commit_view(previous);
                    }
                    self.state = InputState::Command;
                    return Ok(Effect::Continue);
                }
                "A" | "ALL" => return self.commit_view(self.all_view()?),
                "E" | "EXTENTS" => {
                    let bounds = drawing_bounds(&self.drawing)?
                        .ok_or("there is no visible geometry to fit")?;
                    return self.commit_view(centered_fit(bounds, self.view_aspect));
                }
                "C" | "CENTER" => {
                    self.state = InputState::View(ViewInput::Corner { lower_left: false })
                }
                "L" | "LOWER" => {
                    self.state = InputState::View(ViewInput::Corner { lower_left: true })
                }
                "W" | "WINDOW" => self.state = InputState::View(ViewInput::WindowFirst),
                _ => {
                    let upper = line.to_ascii_uppercase();
                    let (value, relative) = upper
                        .strip_suffix('X')
                        .map_or((line, false), |value| (value, true));
                    let factor = number(value.trim())?;
                    if factor <= 0.0 {
                        return Err("zoom factor must be positive".into());
                    }
                    let mut view = if relative {
                        self.drawing.header.view
                    } else {
                        self.all_view()?
                    };
                    view.height /= factor;
                    return self.commit_view(view);
                }
            },
            ViewInput::Corner { lower_left } => {
                let corner = finite_point(point_from(line, self.drawing.header.view.center)?)?;
                self.state = InputState::View(ViewInput::Height { corner, lower_left });
            }
            ViewInput::Height { corner, lower_left } => {
                let height = number(line)?;
                let center = if lower_left {
                    Point {
                        x: corner.x + height * self.view_aspect / 2.0,
                        y: corner.y + height / 2.0,
                    }
                } else {
                    corner
                };
                return self.commit_view(DwgView { center, height });
            }
            ViewInput::WindowFirst => {
                let corner = finite_point(point_from(line, self.drawing.header.view.center)?)?;
                self.state = InputState::View(ViewInput::WindowSecond(corner));
            }
            ViewInput::WindowSecond(first) => {
                let second = finite_point(point_from(line, first)?)?;
                if first.x == second.x || first.y == second.y {
                    return Err("zoom window must have positive width and height".into());
                }
                let bounds = Extents {
                    xmin: first.x.min(second.x),
                    ymin: first.y.min(second.y),
                    xmax: first.x.max(second.x),
                    ymax: first.y.max(second.y),
                };
                return self.commit_view(centered_fit(bounds, self.view_aspect));
            }
            ViewInput::PanFirst => {
                let first = finite_point(point_from(line, Point { x: 0.0, y: 0.0 })?)?;
                self.state = InputState::View(ViewInput::PanSecond {
                    first,
                    relative: line.starts_with('@'),
                });
            }
            ViewInput::PanSecond { first, relative } => {
                let delta = if line.is_empty() && relative {
                    first
                } else {
                    if line.is_empty() {
                        return Err("PAN absolute from point requires a second point".into());
                    }
                    let second = finite_point(point_from(line, first)?)?;
                    Point {
                        x: second.x - first.x,
                        y: second.y - first.y,
                    }
                };
                let view = self.drawing.header.view;
                return self.commit_view(DwgView {
                    center: Point {
                        x: view.center.x - delta.x,
                        y: view.center.y - delta.y,
                    },
                    height: view.height,
                });
            }
        }
        Ok(Effect::Continue)
    }
}
