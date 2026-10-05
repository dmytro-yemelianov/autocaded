//! WebAssembly bindings for AutoRust (AutoCAD-86 1.40).
//! Runs the complete CAD engine and UI presentation in the browser.
//!
//! No corpus bytes are compiled in: the page fetches the font, menu and
//! sample drawings (staged by `scripts/build-wasm.sh`) and hands them over.

use acad_app::{Frame, KeyModifiers, ReportAction, Session};
use acad_model::Drawing;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct AutoCadSession {
    session: Session,
    /// The shape fonts handed to the session, kept for SVG export.
    fonts: acad_render::Libraries,
    viewport_width: u32,
    viewport_height: u32,
}

#[wasm_bindgen]
impl AutoCadSession {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> Result<AutoCadSession, JsValue> {
        let mut session = Session::default();
        let width = width.max(320);
        let height = height.max(240);
        session
            .set_viewport_size(width, height)
            .map_err(|e| JsValue::from_str(&e))?;

        Ok(AutoCadSession {
            session,
            fonts: acad_render::Libraries::default(),
            viewport_width: width,
            viewport_height: height,
        })
    }

    /// Register a shape/font library (e.g. "TXT" from TXT.SHP bytes).
    pub fn load_font(&mut self, name: &str, bytes: &[u8]) -> Result<(), JsValue> {
        self.fonts
            .insert(name, bytes)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.session
            .add_shape_library(name, bytes)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Select how colour numbers are drawn: "pc16" (default) or "aci256".
    pub fn set_palette(&mut self, name: &str) -> Result<(), JsValue> {
        let palette = acad_model::Palette::from_name(name)
            .ok_or_else(|| JsValue::from_str(&format!("unknown palette '{name}'")))?;
        self.session.set_palette(palette);
        self.fonts.set_palette(palette);
        Ok(())
    }

    /// Load a screen menu from MNU bytes (e.g. ACAD.MNU).
    pub fn load_menu(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.session
            .load_menu_bytes(bytes)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Submit a command or response line (like pressing Return in AutoCAD).
    pub fn command(&mut self, input: &str) -> Result<String, JsValue> {
        self.session
            .command(input)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(self.session.prompt().to_owned())
    }

    /// A DOM `keydown`, mapped as the native window maps winit keys: typing
    /// goes to the canvas command area, Return submits it, and the report
    /// viewer takes navigation keys. Returns whether the key was consumed.
    pub fn key_down(
        &mut self,
        key: &str,
        control: bool,
        alt: bool,
        logo: bool,
    ) -> Result<bool, JsValue> {
        let (width, height) = (self.viewport_width, self.viewport_height);
        let result = match key {
            "Enter" if self.session.report_visible() && self.session.input().is_empty() => self
                .session
                .report_action(ReportAction::Close, width, height)
                .map(|_| false),
            "Enter" => {
                let input = self.session.input().to_owned();
                self.session.command(&input)
            }
            _ if self.session.report_visible() && key.chars().count() > 1 => {
                let action = match key {
                    "Escape" => ReportAction::Close,
                    "ArrowUp" => ReportAction::Up,
                    "ArrowDown" => ReportAction::Down,
                    "PageUp" => ReportAction::PageUp,
                    "PageDown" => ReportAction::PageDown,
                    "Home" => ReportAction::Home,
                    "End" => ReportAction::End,
                    _ => return Ok(false),
                };
                self.session
                    .report_action(action, width, height)
                    .map(|_| false)
            }
            "Escape" => self.session.cancel(),
            "Backspace" => {
                let mut input = self.session.input().to_owned();
                input.pop();
                self.session.set_input(input);
                Ok(false)
            }
            // DOM names special keys with words ("Shift", "F1"); text is one char.
            _ if key.chars().count() == 1 => self
                .session
                .key_character(key, KeyModifiers { control, alt, logo }),
            _ => return Ok(false),
        };
        result.map(|_| true).map_err(|e| JsValue::from_str(&e))
    }

    /// Cancel the active command (like pressing Escape).
    pub fn cancel(&mut self) -> Result<bool, JsValue> {
        self.session.cancel().map_err(|e| JsValue::from_str(&e))
    }

    /// Place a coordinate point.
    pub fn point(&mut self, x: f64, y: f64) -> Result<bool, JsValue> {
        self.session
            .point(acad_model::Point { x, y })
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Handle mouse click at client coordinates (points, selection, or menu click).
    pub fn click(
        &mut self,
        client_x: f64,
        client_y: f64,
        width: u32,
        height: u32,
    ) -> Result<bool, JsValue> {
        self.session
            .click(client_x, client_y, width, height)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Handle mouse motion / hover for crosshair and freehand sketch.
    pub fn motion(
        &mut self,
        client_x: f64,
        client_y: f64,
        width: u32,
        height: u32,
    ) -> Result<bool, JsValue> {
        self.session
            .motion(client_x, client_y, width, height)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Update crosshair cursor coordinate.
    pub fn cursor(&mut self, x: f64, y: f64) {
        self.session.cursor(Some((x, y)));
    }

    /// Render current frame to an RGBA8 pixel buffer (width * height * 4 bytes).
    pub fn render_rgba(&mut self, width: u32, height: u32) -> Result<Vec<u8>, JsValue> {
        self.viewport_width = width;
        self.viewport_height = height;
        let _ = self.session.set_viewport_size(width, height);
        let frame: Frame = self
            .session
            .frame(width, height)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(frame.rgba())
    }

    /// Render current frame to PNG bytes.
    pub fn render_png(&mut self, width: u32, height: u32) -> Result<Vec<u8>, JsValue> {
        self.viewport_width = width;
        self.viewport_height = height;
        let _ = self.session.set_viewport_size(width, height);
        let frame: Frame = self
            .session
            .frame(width, height)
            .map_err(|e| JsValue::from_str(&e))?;
        frame.png().map_err(|e| JsValue::from_str(&e))
    }

    /// Open DWG or DXF bytes, chosen by the DWG magic as in the native app
    /// (so a `.BAK` opens as the DWG it is).
    pub fn open_auto(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let drawing = acad_app::decode_drawing(bytes).map_err(|e| JsValue::from_str(&e))?;
        self.session.open_drawing(drawing);
        Ok(())
    }

    /// Open DWG bytes directly.
    pub fn open_dwg(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let drawing = acad_dwg::parse(bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.session.open_drawing(drawing);
        Ok(())
    }

    /// Open DXF text string directly.
    pub fn open_dxf(&mut self, text: &str) -> Result<(), JsValue> {
        let drawing =
            acad_dxf::parse(text.as_bytes()).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.session.open_drawing(drawing);
        Ok(())
    }

    /// Export drawing to 1983 DWG binary bytes (version: "1.4" or "1.2").
    pub fn export_dwg(&self, version: &str) -> Result<Vec<u8>, JsValue> {
        let ver = match version.trim() {
            "1.2" | "ac1.2" | "AC1.2" => acad_dwg::header::Version::Ac12,
            _ => acad_dwg::header::Version::Ac140,
        };
        acad_dwg::write_version(self.session.drawing(), ver)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Export drawing to authentic 1983 DXF text.
    pub fn export_dxf(&self) -> Result<String, JsValue> {
        let bytes = acad_dxf::try_write(self.session.drawing())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Export drawing to modern vector SVG.
    pub fn export_svg(&self, width: u32, height: u32) -> Result<String, JsValue> {
        Ok(drawing_to_svg(
            self.session.drawing(),
            &self.fonts,
            width,
            height,
        ))
    }

    /// Get detailed session state as JSON string.
    pub fn get_state_json(&self) -> String {
        let d = self.session.drawing();
        let h = &d.header;
        let obj = serde_json::json!({
            "prompt": self.session.prompt(),
            "status": self.session.status(),
            "input": self.session.input(),
            "dirty": self.session.is_dirty(),
            "report_visible": self.session.report_visible(),
            "report_text": self.session.report_text(),
            "entities": d.entities().count(),
            "current_layer": h.current_layer,
            "snap": { "on": h.snap.on, "spacing": h.snap.spacing },
            "grid": { "on": h.grid.on, "spacing": h.grid.spacing },
            "ortho": h.ortho,
            "limits": { "xmin": h.limits.xmin, "ymin": h.limits.ymin, "xmax": h.limits.xmax, "ymax": h.limits.ymax },
            "extents": { "xmin": h.extents.xmin, "ymin": h.extents.ymin, "xmax": h.extents.xmax, "ymax": h.extents.ymax },
        });
        obj.to_string()
    }

    /// Navigate full-screen report viewer (STATUS, LIST, DBLIST, HELP).
    /// Actions: "up", "down", "page_up", "page_down", "home", "end", "close".
    pub fn report_action(&mut self, action: &str, width: u32, height: u32) -> Result<(), JsValue> {
        let act = match action {
            "up" => ReportAction::Up,
            "down" => ReportAction::Down,
            "page_up" => ReportAction::PageUp,
            "page_down" => ReportAction::PageDown,
            "home" => ReportAction::Home,
            "end" => ReportAction::End,
            "close" => ReportAction::Close,
            _ => return Err(JsValue::from_str("invalid report action")),
        };
        let _ = self.session.report_action(act, width, height);
        Ok(())
    }

    /// Reset to an empty drawing.
    pub fn reset(&mut self) {
        self.session.reset();
    }
}

/// Convert drawing geometry to standalone, scalable SVG.
fn drawing_to_svg(
    drawing: &Drawing,
    libraries: &acad_render::Libraries,
    width: u32,
    height: u32,
) -> String {
    let bounds = if (drawing.header.extents.xmax - drawing.header.extents.xmin).abs() > 1e-4 {
        &drawing.header.extents
    } else {
        &drawing.header.limits
    };
    let vp = acad_render::Viewport::fit(bounds, width.max(100), height.max(100));
    let output = acad_render::flatten_with_libraries(drawing, &vp, libraries);

    let mut svg = String::with_capacity(output.primitives.len() * 128 + 256);
    svg.push_str(&format!(
        r#"<svg viewBox="0 0 {width} {height}" width="{width}" height="{height}" xmlns="http://www.w3.org/2000/svg" style="background:#000000;font-family:monospace">"#
    ));
    svg.push('\n');
    svg.push_str(r#"  <!-- Generated by AutoRust (AutoCAD-86 1.40 in Rust) -->"#);
    svg.push('\n');

    let white = "#ffffff";
    for prim in output.primitives {
        match prim {
            acad_render::Prim::Polyline(points) => {
                let pts: String = points
                    .iter()
                    .map(|p| format!("{:.1},{:.1} ", p.x, p.y))
                    .collect();
                svg.push_str(&format!(
                    r#"  <polyline points="{pts}" fill="none" stroke="{white}" stroke-width="1"/>"#
                ));
                svg.push('\n');
            }
            acad_render::Prim::ColoredPolyline { points, rgb } => {
                let pts: String = points
                    .iter()
                    .map(|p| format!("{:.1},{:.1} ", p.x, p.y))
                    .collect();
                let hex = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
                svg.push_str(&format!(
                    r#"  <polyline points="{pts}" fill="none" stroke="{hex}" stroke-width="1"/>"#
                ));
                svg.push('\n');
            }
            acad_render::Prim::FilledPolygon(points) => {
                let pts: String = points
                    .iter()
                    .map(|p| format!("{:.1},{:.1} ", p.x, p.y))
                    .collect();
                svg.push_str(&format!(
                    r#"  <polygon points="{pts}" fill="{white}" stroke="{white}" stroke-width="1"/>"#
                ));
                svg.push('\n');
            }
            acad_render::Prim::ColoredFilledPolygon { points, rgb } => {
                let pts: String = points
                    .iter()
                    .map(|p| format!("{:.1},{:.1} ", p.x, p.y))
                    .collect();
                let hex = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                svg.push_str(&format!(
                    r#"  <polygon points="{pts}" fill="{hex}" stroke="{hex}" stroke-width="1"/>"#
                ));
                svg.push('\n');
            }
        }
    }

    svg.push_str("</svg>\n");
    svg
}

// Native tests cover the paths that never build a `JsValue` (which panics
// off wasm32): successful typing, opening, exporting and palette choice.
#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> AutoCadSession {
        AutoCadSession::new(800, 600).expect("session")
    }

    fn state(cad: &AutoCadSession) -> serde_json::Value {
        serde_json::from_str(&cad.get_state_json()).unwrap()
    }

    fn type_line(cad: &mut AutoCadSession, line: &str) {
        for key in line.chars() {
            assert!(cad.key_down(&key.to_string(), false, false, false).unwrap());
        }
        assert!(cad.key_down("Enter", false, false, false).unwrap());
    }

    #[test]
    fn typed_keys_reach_the_command_area_and_draw() {
        let mut cad = session();
        assert!(cad.key_down("L", false, false, false).unwrap());
        assert!(cad.key_down("X", false, false, false).unwrap());
        assert!(cad.key_down("Backspace", false, false, false).unwrap());
        assert_eq!(state(&cad)["input"], "L");
        assert!(cad.key_down("Escape", false, false, false).unwrap());
        assert!(!cad.key_down("Shift", false, false, false).unwrap());

        type_line(&mut cad, "LINE");
        type_line(&mut cad, "1,1");
        type_line(&mut cad, "4,3");
        type_line(&mut cad, "");
        assert_eq!(state(&cad)["entities"], 1);
        assert_eq!(state(&cad)["dirty"], true);
    }

    #[test]
    fn open_auto_reads_its_own_dwg_and_dxf_exports() {
        let mut cad = session();
        type_line(&mut cad, "CIRCLE");
        type_line(&mut cad, "5,5");
        type_line(&mut cad, "2");
        let dwg = cad.export_dwg("1.4").unwrap();
        let dxf = cad.export_dxf().unwrap();

        for bytes in [dwg, dxf.into_bytes()] {
            let mut reopened = session();
            reopened.open_auto(&bytes).unwrap();
            assert_eq!(state(&reopened)["entities"], 1);
            assert_eq!(state(&reopened)["dirty"], false);
        }
    }

    #[test]
    fn palette_names_select_the_session_palette() {
        let mut cad = session();
        assert_eq!(cad.session.palette(), acad_model::Palette::Pc16);
        cad.set_palette("aci256").unwrap();
        assert_eq!(cad.session.palette(), acad_model::Palette::Aci256);
        assert_eq!(cad.fonts.palette(), acad_model::Palette::Aci256);
    }
}
