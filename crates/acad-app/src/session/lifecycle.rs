//! Document path, semantic save baseline and exit behavior for every transport.
use super::*;
use std::path::Path;

/// Status row at the SAVE/END member-erasure question; its start fits an
/// 800-pixel window, the API `state` status carries all of it.
pub(crate) const ORIGINAL_ERASURE_EXPLANATION: &str = "Erased REPEAT group holds earlier erased members; Y writes every member erased, as AutoCAD 1.4 does (their earlier erasure is lost); anything else writes nothing";

impl Session {
    pub fn document_path(&self) -> Option<&Path> {
        self.document.path.as_deref()
    }
    pub fn document_format(&self) -> Option<&'static str> {
        self.document.format.map(Format::name)
    }
    pub fn is_dirty(&self) -> bool {
        self.document.dirty(self.drawing())
    }

    /// A drawing dropped on the window (native policy): opens it like API
    /// `open`, keeping the script clock, Main Menu policy, palette and font
    /// directories, unless the current drawing has unsaved changes, which a
    /// stray drop must not discard. Refusals and decode errors go to the
    /// status line; the current session is unchanged.
    pub fn open_dropped(&mut self, path: &Path) -> Result<bool, String> {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        if self.main_menu.is_none() && self.is_dirty() {
            self.status = format!("Unsaved changes: SAVE or END before opening {name}");
            return Ok(false);
        }
        match Session::open(path, &self.directories) {
            Ok(mut opened) => {
                opened.inherit_main_menu_home(self);
                opened.set_script_clock(self.script_clock());
                opened.set_viewport_size(self.viewport_size.0, self.viewport_size.1)?;
                opened.status = format!("Opened {name}");
                *self = opened;
            }
            Err(error) => self.status = format!("Cannot open {name}: {error}"),
        }
        Ok(false)
    }

    /// Explicit Save As: choose codec by suffix, and attach only after success.
    /// Replacing an existing file keeps its previous bytes as `.BAK`
    /// (docs/native-files-menu.md); failures leave file and document as-is.
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        self.refuse_at_main_menu()?;
        let path = std::path::absolute(path).map_err(|e| format!("Save failed: {e}"))?;
        let format = Format::for_path(&path);
        self.save_as_format(&path, format)
    }
    fn save_as_format(&mut self, path: &Path, format: Format) -> Result<(), String> {
        let warning = format
            .save(path, self.drawing())
            .map_err(|e| format!("Save failed: {e}"))?;
        self.document = Document::saved(path.to_path_buf(), format, self.drawing());
        self.status = match warning {
            Some(warning) => format!("Saved {}; {warning}", path.display()),
            None => format!("Saved {}", path.display()),
        };
        Ok(())
    }
    /// SAVE/END to DWG with an ambiguous erased REPEAT owner asks the
    /// editor's AutoCAD 1.4 member-erasure question instead of refusing
    /// (docs/native-group-persistence.md). `path` is the SAVE/END output,
    /// `None` the attached document. Returns whether the question is open;
    /// nothing has been written then.
    pub(crate) fn ask_original_erasure(&mut self, path: Option<String>, quit: bool) -> bool {
        let format = match &path {
            Some(path) => Format::for_path(Path::new(path)),
            None => match self.document.format {
                Some(format) => format,
                None => return false,
            },
        };
        if !matches!(format, Format::Dwg(_))
            || !acad_model::group_codec::has_ambiguous_owner(self.drawing())
        {
            return false;
        }
        self.editor.ask_original_erasure_save(path, quit);
        self.status = ORIGINAL_ERASURE_EXPLANATION.into();
        true
    }
    /// A command script item or menu-macro piece that answers the question
    /// with anything but `Y` (docs/native-scripts.md): the question is native
    /// only, so scripts and macros written for the original carry no answer
    /// and their next item would be taken as one. The decline becomes an
    /// error, which interrupts a script (RESUME continues after the item)
    /// and ends a macro, keeping the "nothing written" message visible.
    pub(crate) fn automated_erasure_answer(
        &self,
        asked: bool,
        result: Result<acad_cmd::Effect, String>,
    ) -> Result<acad_cmd::Effect, String> {
        if asked && matches!(result, Ok(acad_cmd::Effect::Continue)) {
            Err(self.editor.status().to_owned())
        } else {
            result
        }
    }
    /// The confirmed answer: write AutoCAD 1.4's form of each ambiguous
    /// erased owner. The session drawing, its OOPS/UNDO history and the
    /// failure behaviour of an ordinary save are unchanged; the written
    /// drawing becomes the saved baseline of the session drawing.
    pub(crate) fn save_original_erasure(&mut self, path: Option<&str>) -> Result<(), String> {
        let (path, format) = match path {
            Some(path) => {
                self.refuse_at_main_menu()?;
                let path = std::path::absolute(path).map_err(|e| format!("Save failed: {e}"))?;
                let format = Format::for_path(&path);
                (path, format)
            }
            None => match (self.document.path.clone(), self.document.format) {
                (Some(path), Some(format)) => (path, format),
                _ => return Err("Save failed: no attached drawing file".into()),
            },
        };
        let (converted, count) = acad_model::group_codec::original_member_erasure(self.drawing());
        let warning = format
            .save(&path, &converted)
            .map_err(|e| format!("Save failed: {e}"))?;
        self.document = Document::saved(path.clone(), format, self.drawing());
        let mut status = format!(
            "Saved {}; {count} erased REPEAT group(s) written as AutoCAD 1.4 does, every member erased (earlier member erasure not kept)",
            path.display()
        );
        if let Some(warning) = warning {
            status = format!("{status}; {warning}");
        }
        self.status = status;
        Ok(())
    }
    pub(crate) fn end(&mut self) -> Result<bool, String> {
        let Some(path) = self.document.path.clone() else {
            self.editor.request_end_path();
            self.status.clear();
            return Ok(false);
        };
        let format = self.document.format.expect("attached document has a codec");
        if self.ask_original_erasure(None, true) {
            return Ok(false);
        }
        // Preserve the detected source codec/revision even when its suffix is
        // misleading or .BAK. END writes even when the baseline is pristine.
        self.save_as_format(&path, format)?;
        Ok(self.leave_editor())
    }
    /// Native QUIT and window close both enter the editor's Y/YES dialogue.
    /// At the Main Menu (nothing unsaved) it exits at once; in the editor
    /// its Y answer exits the process even when QUIT would return to the
    /// Main Menu (docs/native-main-menu.md).
    pub fn request_quit(&mut self) -> Result<bool, String> {
        if self.main_menu.is_some() {
            return Ok(true);
        }
        self.exit_requested = true;
        self.interrupt_script(ScriptInterrupt::Input);
        self.input.clear();
        let result = self.editor.request_quit();
        self.apply_effect(result)
    }
}
