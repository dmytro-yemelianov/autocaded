//! Document path, semantic save baseline and exit behavior for every transport.
use super::*;
use std::path::Path;

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
    pub(crate) fn end(&mut self) -> Result<bool, String> {
        let Some(path) = self.document.path.clone() else {
            self.editor.request_end_path();
            self.status.clear();
            return Ok(false);
        };
        let format = self.document.format.expect("attached document has a codec");
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
