//! Point-placement policy for the native mouse. Typed coordinates bypass it.

use crate::{input_state::InputState, Editor};
use acad_model::Point;

impl Editor {
    /// Resolve a mouse point without advancing the command or modifying drawing
    /// data. The application uses this for the crosshair and submission uses the
    /// same calculation. SNAP uses the world-origin grid; ORTHO then retains the
    /// anchor's exact coordinate on the locked axis, including typed anchors that
    /// lie between grid lines. Horizontal wins equal displacements.
    ///
    /// View/navigation and selection-window points remain unconstrained. Arcs,
    /// dimension origins and two-axis settings can snap but have no ORTHO anchor.
    /// These are native-app policies, not a claimed original digitizer emulation.
    pub fn constrain_mouse_point(&self, mut point: Point) -> Result<Point, String> {
        if !self.accepts_mouse_point() {
            return Err("current prompt does not accept a point".into());
        }
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err("mouse coordinates must be finite".into());
        }
        if self.selection_window_pending() || matches!(self.state, InputState::View(_)) {
            return Ok(point);
        }
        point = self.snap_mouse_point(point)?;
        if self.drawing.header.ortho {
            if let Some(anchor) = self.state.ortho_origin() {
                let dx = point.x - anchor.x;
                let dy = point.y - anchor.y;
                if !dx.is_finite() || !dy.is_finite() {
                    return Err("ORTHO displacement exceeds the finite coordinate range".into());
                }
                if dx.abs() >= dy.abs() {
                    point.y = anchor.y;
                } else {
                    point.x = anchor.x;
                }
            }
        }
        Ok(point)
    }
}

impl Editor {
    /// The SNAP step of `constrain_mouse_point` alone.
    pub(crate) fn snap_mouse_point(&self, point: Point) -> Result<Point, String> {
        let snap = self.drawing.header.snap;
        if !snap.on {
            return Ok(point);
        }
        if !snap.spacing.is_finite() || snap.spacing <= 0.0 {
            return Err("SNAP spacing must be positive and finite".into());
        }
        Ok(Point {
            x: snap_coordinate(point.x, snap.spacing)?,
            y: snap_coordinate(point.y, snap.spacing)?,
        })
    }
}

fn snap_coordinate(value: f64, spacing: f64) -> Result<f64, String> {
    // f64::round chooses the nearest grid point, with half-steps away from zero.
    let snapped = (value / spacing).round() * spacing;
    if !snapped.is_finite() {
        return Err("SNAP coordinate exceeds the finite grid range".into());
    }
    Ok(snapped)
}

impl InputState {
    fn ortho_origin(&self) -> Option<Point> {
        match self {
            Self::LineNext { previous, .. } => Some(*previous),
            Self::SecondPoint(_, point)
            | Self::DistanceSecondPoint(point)
            | Self::SolidSecondPoint(point)
            | Self::SolidThirdPoint(_, point)
            | Self::SolidFourthPoint(_, _, point) => Some(*point),
            Self::TraceNext(_, points) | Self::AreaNextPoint(points) => points.last().copied(),
            Self::Sketch(sketch) => sketch.ortho_origin(),
            _ => None,
        }
    }
}
