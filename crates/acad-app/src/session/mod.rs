//! Shared application session; window ownership and transports live elsewhere.
use crate::presentation::*;
use crate::{
    command_line,
    document::{decode_drawing, save_dwg, Document, Format},
    files, menu_panel,
};
use acad_render::Libraries;
mod commands;
mod effects;
mod external_insert;
mod hatch_pattern;
mod lifecycle;
mod menu_macro;
mod mouse;
mod resources;
mod script;
use resources::*;
pub use script::{
    ScriptClock, ScriptInterrupt, ScriptPhase, ScriptPump, SystemClock, MAX_SCRIPT_BYTES,
    SCRIPT_ITEMS_PER_PUMP,
};

/// Modifier keys held with a typed character (the GUI keyboard seam).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub control: bool,
    pub alt: bool,
    /// Command/Windows/Super.
    pub logo: bool,
}

pub struct Session {
    pub(crate) editor: acad_cmd::Editor,
    pub(crate) libraries: Libraries,
    pub(crate) menu: Option<acad_cmd::menu::MenuFile>,
    pub(crate) menu_page: usize,
    pub(crate) input: String,
    pub(crate) status: String,
    pub(crate) report: Option<crate::report_view::Report>,
    pub(crate) cursor: Option<(f64, f64)>,
    pub(crate) document: Document,
    viewport_size: (u32, u32),
    /// Source path of the most recently staged INSERT file.
    pub(crate) pending_insert_source: Option<std::path::PathBuf>,
    /// The one command script, if any (docs/native-scripts.md).
    script: Option<script::ScriptRun>,
    /// True while a script item is being submitted.
    script_stepping: bool,
    script_clock: std::sync::Arc<dyn ScriptClock>,
    /// Remaining steps of a screen-menu macro waiting at a `\` pause.
    pub(crate) paused_macro: Option<std::collections::VecDeque<acad_cmd::menu::MacroStep>>,
}

#[cfg(test)]
mod script_tests;
#[cfg(test)]
mod sketch_tests;
#[cfg(test)]
mod tests;

impl Default for Session {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl Session {
    pub fn new(directories: &[std::path::PathBuf]) -> Self {
        let mut search = directories.to_vec();
        let system = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/System");
        if system.is_dir() {
            search.push(system);
        }
        let (libraries, diagnostics) = Libraries::load(&search);
        let mut session = Self::with_editor(acad_cmd::Editor::default(), libraries);
        if !diagnostics.is_empty() {
            session.status = diagnostics.join("; ");
        }
        session
    }
    fn with_editor(editor: acad_cmd::Editor, libraries: Libraries) -> Self {
        let document = Document::unnamed(editor.drawing());
        let mut session = Self {
            editor,
            libraries,
            menu: None,
            menu_page: 0,
            input: String::new(),
            status: String::new(),
            report: None,
            cursor: None,
            document,
            viewport_size: (800, 600),
            pending_insert_source: None,
            script: None,
            script_stepping: false,
            script_clock: std::sync::Arc::new(SystemClock::default()),
            paused_macro: None,
        };
        session
            .set_viewport_size(800, 600)
            .expect("default viewport is positive");
        session.register_libraries();
        session.load_menu("ACAD");
        session
    }
    fn register_libraries(&mut self) {
        self.editor.register_text_metrics(std::sync::Arc::new(
            crate::text_metrics::RuntimeTextMetrics(self.libraries.clone()),
        ));
        for (name, library) in self.libraries.iter() {
            self.editor.register_shape_library(
                name,
                library
                    .named_shapes()
                    .map(|(shape, number)| (shape.to_owned(), number)),
            );
        }
    }
    /// Set the physical client size used by navigation; command area is excluded.
    pub fn set_viewport_size(&mut self, width: u32, height: u32) -> Result<(), String> {
        if width == 0 || height == 0 {
            return Err("client dimensions must be positive".into());
        }
        self.editor
            .set_viewport_size(width, command_line::drawing_height(height).max(1))?;
        self.viewport_size = (width, height);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.editor = acad_cmd::Editor::default();
        self.set_viewport_size(self.viewport_size.0, self.viewport_size.1)
            .expect("retained viewport is positive");
        self.document = Document::unnamed(self.editor.drawing());
        self.pending_insert_source = None;
        self.script = None;
        self.abandon_macro();
        self.register_libraries();
        self.input.clear();
        self.status.clear();
        self.report = None;
        self.cursor = None;
    }

    pub fn open(
        path: &std::path::Path,
        directories: &[std::path::PathBuf],
    ) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let drawing = decode_drawing(&bytes)?;
        let path = std::path::absolute(path).map_err(|e| e.to_string())?;
        let format = Format::from_bytes(&bytes);
        let (libraries, diagnostics) = Libraries::for_drawing(&path, directories);
        let mut session = Self::with_editor(acad_cmd::Editor::new(drawing), libraries);
        session.document = Document::saved(path, format, session.drawing());
        if !diagnostics.is_empty() {
            session.status = diagnostics.join("; ");
        }
        Ok(session)
    }

    /// The same Return route as the window, including SHP resolution and effects.
    /// User input interrupts an active command script first.
    pub fn command(&mut self, input: &str) -> Result<bool, String> {
        self.interrupt_script(ScriptInterrupt::Input);
        self.submit_line(input)
    }

    /// One Return-terminated line through the shared route (window, API and
    /// script items alike).
    fn submit_line(&mut self, input: &str) -> Result<bool, String> {
        self.input = input.to_owned();
        let mut outcome = None;
        let mut quit = false;
        self.submit_return_input(&mut |session, result| {
            let applied = session.apply_effect(result);
            quit |= matches!(applied, Ok(true));
            // The user's own submission decides the result; a resumed
            // menu macro's later Returns only report through status.
            outcome.get_or_insert(applied);
        });
        if quit {
            return Ok(true);
        }
        outcome.unwrap_or_else(|| Err(self.status.clone()))
    }

    /// A key press carrying text. While SKETCH is active, Ctrl+C cancels (as
    /// in the original) and any other Ctrl/Alt/logo chord is ignored, so a
    /// chord never acts as an immediate SKETCH control. Elsewhere, control
    /// characters are dropped and text goes to `type_characters`.
    pub fn key_character(&mut self, text: &str, modifiers: KeyModifiers) -> Result<bool, String> {
        if self.editor.sketch_active() && (modifiers.control || modifiers.alt || modifiers.logo) {
            let ctrl_c = matches!(text, "c" | "C" | "\u{3}");
            if modifiers.control && !modifiers.alt && !modifiers.logo && ctrl_c {
                return self.cancel();
            }
            return Ok(false);
        }
        if text.chars().any(char::is_control) {
            return Ok(false);
        }
        self.type_characters(text)
    }

    /// Physical character input. Idle Space repeats like Return, while spaces
    /// in TEXT payloads and other editable input remain significant characters.
    pub fn type_characters(&mut self, text: &str) -> Result<bool, String> {
        if !text.is_empty() {
            self.interrupt_script(ScriptInterrupt::Input);
        }
        // SKETCH controls act on the key press, as on the original keyboard.
        if self.editor.sketch_active() && self.input.is_empty() {
            for key in text.chars().filter(|c| !c.is_whitespace()) {
                if self.command(&key.to_string())? {
                    return Ok(true);
                }
                if !self.editor.sketch_active() {
                    break;
                }
            }
            return Ok(false);
        }
        if text == " " && self.input.is_empty() && self.editor.prompt() == "Command" {
            return self.command("");
        }
        self.input.push_str(text);
        Ok(false)
    }

    pub fn point(&mut self, point: acad_model::Point) -> Result<bool, String> {
        self.interrupt_script(ScriptInterrupt::Input);
        if self.editor.accepts_mouse_selection() {
            let picked = self.editor.pick_selection_at(point, 0.0)?;
            self.input = self
                .editor
                .collected_selection()
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            self.status = picked.map_or_else(
                || "No entity at pick point".into(),
                |id| format!("Selected entity {id}"),
            );
            return Ok(false);
        }
        let result = self.editor.submit_mouse_point(point);
        if result.is_ok() {
            self.input.clear();
        }
        let outcome = self.apply_effect(result);
        let mut quit = false;
        self.resume_macro(&mut |session, result| {
            quit |= matches!(session.apply_effect(result), Ok(true));
        });
        if quit {
            Ok(true)
        } else {
            outcome
        }
    }

    pub fn cancel(&mut self) -> Result<bool, String> {
        self.interrupt_script(ScriptInterrupt::Cancel);
        self.input.clear();
        self.abandon_macro();
        let result = self.editor.cancel_command();
        self.apply_effect(result)
    }

    pub fn click(&mut self, x: f64, y: f64, width: u32, height: u32) -> Result<bool, String> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= f64::from(width)
            || y >= f64::from(height)
        {
            return Err("click must be inside the frame".into());
        }
        self.interrupt_script(ScriptInterrupt::Input);
        self.cursor = Some((x, y));
        let mut quit = false;
        let mut error = None;
        self.handle_left_click(width, height, |session, result| {
            match session.apply_effect(result) {
                Ok(requested) => quit |= requested,
                Err(failure) => {
                    error.get_or_insert(failure);
                }
            }
        });
        if quit {
            Ok(true)
        } else if let Some(error) = error {
            Err(error)
        } else {
            Ok(false)
        }
    }

    pub fn drawing(&self) -> &acad_model::Drawing {
        self.editor.drawing()
    }
    pub fn prompt(&self) -> &str {
        self.editor.prompt()
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn input(&self) -> &str {
        &self.input
    }
    pub fn set_input(&mut self, input: String) {
        if input != self.input {
            self.interrupt_script(ScriptInterrupt::Input);
        }
        if !input.is_empty() {
            if let Some(report) = &mut self.report {
                report.hide();
            }
        }
        self.input = input;
    }
    pub fn report_text(&self) -> Option<&str> {
        self.report.as_ref().map(crate::report_view::Report::text)
    }
    pub fn report_visible(&self) -> bool {
        self.report
            .as_ref()
            .is_some_and(crate::report_view::Report::visible)
    }
    /// View controls never submit editor input or change the drawing/undo state.
    pub fn report_action(
        &mut self,
        action: crate::ReportAction,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let report = self.report.as_mut().ok_or("No report available")?;
        report.navigate(action, width, height);
        Ok(())
    }
    pub fn cursor(&mut self, point: Option<(f64, f64)>) {
        self.cursor = point;
    }
    pub fn title(&self) -> String {
        let menu = self
            .menu
            .as_ref()
            .map(|m| format!(" | screen menu: {} entries", m.entries.len()))
            .unwrap_or_default();
        format!(
            "AutoCAD 1.4 — {}{} | {} {} {}{}",
            self.document
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy())
                .unwrap_or("Untitled".into()),
            if self.is_dirty() { " *" } else { "" },
            self.prompt(),
            self.input,
            self.status,
            menu
        )
    }
    pub fn minimum_size(&self) -> (u32, u32) {
        let size = supported_client_size(self.menu.as_ref(), winit::dpi::PhysicalSize::new(0, 0));
        (size.width, size.height)
    }
}
impl Session {
    /// API pointer motion: the same validation as `click`, then the shared seam.
    pub fn motion(&mut self, x: f64, y: f64, width: u32, height: u32) -> Result<bool, String> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= f64::from(width)
            || y >= f64::from(height)
        {
            return Err("motion must be inside the frame".into());
        }
        self.cursor = Some((x, y));
        self.pointer_motion(width, height)
    }

    pub fn sketch_active(&self) -> bool {
        self.editor.sketch_active()
    }

    pub fn click_cursor(&mut self, width: u32, height: u32) -> Result<bool, String> {
        if let Some((x, y)) = self.cursor {
            self.click(x, y, width, height)
        } else {
            Ok(false)
        }
    }
}
