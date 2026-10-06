//! Execute editor effects once, with errors visible to both GUI and API.
use super::*;

impl Session {
    #[cfg(test)]
    pub(crate) fn apply_result(&mut self, result: Result<acad_cmd::Effect, String>) -> bool {
        self.apply_effect(result).unwrap_or(false)
    }

    pub(crate) fn apply_effect(
        &mut self,
        result: Result<acad_cmd::Effect, String>,
    ) -> Result<bool, String> {
        let diagnostic = self.submission_diagnostic.take();
        self.clear_status();
        self.merge_committed_insert_libraries();
        let outcome = self.perform_effect(result);
        if let Err(error) = &outcome {
            self.set_status(error.clone());
            if let Some(diagnostic) = diagnostic {
                self.rendered_status = diagnostic.message.render(self.locale);
                self.status_diagnostic = Some(diagnostic);
            }
            self.sync_collected_selection();
        }
        outcome
    }

    /// Mirror a retryable collection after either successful collection or
    /// geometry/cardinality rejection, so the shared highlights keep its owners.
    fn sync_collected_selection(&mut self) {
        if self.editor.accepts_mouse_selection() && !self.editor.collected_selection().is_empty() {
            self.input = self
                .editor
                .collected_selection()
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
        }
    }

    fn perform_effect(&mut self, result: Result<acad_cmd::Effect, String>) -> Result<bool, String> {
        self.report = None;
        match result? {
            acad_cmd::Effect::Continue => {
                self.set_status(self.editor.status().to_owned());
                self.sync_collected_selection();
            }
            acad_cmd::Effect::Quit => return Ok(self.leave_editor()),
            acad_cmd::Effect::End => return self.end(),
            acad_cmd::Effect::SaveAndQuit(path) => {
                if !self.ask_original_erasure(Some(path.clone()), true) {
                    self.save(std::path::Path::new(&path))?;
                    return Ok(self.leave_editor());
                }
            }
            acad_cmd::Effect::UnloadMenu => self.unload_menu(),
            acad_cmd::Effect::Save(path) => {
                if !self.ask_original_erasure(Some(path.clone()), false) {
                    self.save(std::path::Path::new(&path))?;
                }
            }
            acad_cmd::Effect::SaveOriginalErasure { path, quit } => {
                self.save_original_erasure(path.as_deref())?;
                if quit {
                    return Ok(self.leave_editor());
                }
            }
            acad_cmd::Effect::CheckWblockDestination(path) => {
                let open = self.document.path.as_deref();
                match crate::document::wblock_destination(std::path::Path::new(&path), open) {
                    Ok(WblockDestination::New) => {}
                    Ok(WblockDestination::Existing) => self.editor.ask_wblock_replace(false),
                    Ok(WblockDestination::OpenDrawing) => self.editor.ask_wblock_replace(true),
                    Err(e) => {
                        self.editor.cancel_command()?;
                        return Err(format!("WBLOCK failed: {e}"));
                    }
                }
                self.set_status(self.editor.status().to_owned());
            }
            acad_cmd::Effect::SaveDrawing(path, drawing) => {
                self.write_wblock(&path, &drawing, false)?
            }
            acad_cmd::Effect::ReplaceDrawing(path, drawing) => {
                self.write_wblock(&path, &drawing, true)?
            }
            acad_cmd::Effect::LoadMenu(requested) => {
                self.try_load_menu(&requested)?;
            }
            acad_cmd::Effect::Files(request) => {
                let report = files::execute(request)?;
                eprintln!("{report}");
                self.set_status(report.lines().next().unwrap_or("FILES complete").to_owned());
                self.report = Some(crate::report_view::Report::new(report));
            }
            effect @ (acad_cmd::Effect::Delay(_)
            | acad_cmd::Effect::Script(_)
            | acad_cmd::Effect::Resume) => self.perform_script_effect(effect)?,
            acad_cmd::Effect::Report(report) => {
                eprintln!("{report}");
                self.set_status(if self.editor.status().is_empty() {
                    "Report: PgUp/PgDn, wheel; Esc closes".into()
                } else {
                    self.editor.status().to_owned()
                });
                self.report = Some(crate::report_view::Report::new(report));
            }
        }
        Ok(false)
    }
}

impl Session {
    /// Write WBLOCK output. When it lands on the attached document's own
    /// file (only after the open-drawing question, or a file that did not
    /// exist), the document stays attached with that output as its saved
    /// baseline, so dirty state, QUIT and END reflect what the file holds.
    fn write_wblock(
        &mut self,
        path: &str,
        drawing: &acad_model::Drawing,
        replace: bool,
    ) -> Result<(), String> {
        let warning =
            save_dwg(path, drawing, replace).map_err(|e| format!("WBLOCK failed: {e}"))?;
        let mut status = format!("Wrote block drawing {path}");
        let document = self.document.path.clone();
        if let Some(document) = document.filter(|document| {
            crate::document::is_document_file(std::path::Path::new(path), Some(document))
        }) {
            self.document = Document::saved(
                document,
                Format::Dwg(acad_dwg::header::Version::Ac140),
                drawing,
            );
            status.push_str(" over the open drawing's file");
        }
        if let Some(warning) = warning {
            status = format!("{status}; {warning}");
        }
        self.set_status(status);
        Ok(())
    }
}
