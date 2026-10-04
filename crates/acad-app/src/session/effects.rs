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
        self.merge_committed_insert_libraries();
        let outcome = self.perform_effect(result);
        if let Err(error) = &outcome {
            self.status = error.clone();
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
                self.status = self.editor.status().to_owned();
                self.sync_collected_selection();
            }
            acad_cmd::Effect::Quit => return Ok(true),
            acad_cmd::Effect::End => return self.end(),
            acad_cmd::Effect::SaveAndQuit(path) => {
                self.save(std::path::Path::new(&path))?;
                return Ok(true);
            }
            acad_cmd::Effect::UnloadMenu => self.unload_menu(),
            acad_cmd::Effect::Save(path) => {
                self.save(std::path::Path::new(&path))?;
            }
            acad_cmd::Effect::SaveDrawing(path, drawing) => {
                let warning =
                    save_dwg(&path, &drawing).map_err(|e| format!("WBLOCK failed: {e}"))?;
                self.status = match warning {
                    Some(warning) => format!("Wrote block drawing {path}; {warning}"),
                    None => format!("Wrote block drawing {path}"),
                };
            }
            acad_cmd::Effect::LoadMenu(requested) => {
                self.try_load_menu(&requested)?;
            }
            acad_cmd::Effect::Files(request) => {
                let report = files::execute(request)?;
                eprintln!("{report}");
                self.status = report.lines().next().unwrap_or("FILES complete").to_owned();
                self.report = Some(crate::report_view::Report::new(report));
            }
            effect @ (acad_cmd::Effect::Delay(_)
            | acad_cmd::Effect::Script(_)
            | acad_cmd::Effect::Resume) => self.perform_script_effect(effect)?,
            acad_cmd::Effect::Report(report) => {
                eprintln!("{report}");
                self.status = if self.editor.status().is_empty() {
                    "Report: PgUp/PgDn, wheel; Esc closes".into()
                } else {
                    self.editor.status().to_owned()
                };
                self.report = Some(crate::report_view::Report::new(report));
            }
        }
        Ok(false)
    }
}
