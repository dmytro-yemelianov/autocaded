//! Files prompt handling, extracted without changing command policy.
use super::*;

impl Editor {
    pub(super) fn submit_files(&mut self, state: InputState, line: &str) -> Result<Effect, String> {
        match state {
            InputState::SavePath | InputState::EndSavePath => {
                if line.is_empty() {
                    return Err("output path cannot be empty".into());
                }
                self.state = InputState::Command;
                Ok(if matches!(state, InputState::EndSavePath) {
                    Effect::SaveAndQuit(line.to_owned())
                } else {
                    Effect::Save(line.to_owned())
                })
            }
            InputState::OriginalErasureSave { path, quit } => {
                // As WBLOCK's replace question: an answer starting with Y
                // confirms; anything else, including Return, writes nothing.
                self.state = InputState::Command;
                if line.trim_start().starts_with(['Y', 'y']) {
                    return Ok(Effect::SaveOriginalErasure { path, quit });
                }
                self.status = format!(
                    "{}: nothing written; the erased REPEAT group keeps its members' earlier erasure in this session (OOPS or UNDO before saving)",
                    if quit { "END" } else { "SAVE" }
                );
                Ok(Effect::Continue)
            }
            InputState::QuitConfirmation => {
                let suspended = self.suspended_sketch.take();
                if line.eq_ignore_ascii_case("Y") || line.eq_ignore_ascii_case("YES") {
                    self.state = InputState::Command;
                    return Ok(Effect::Quit);
                }
                // Declining resumes a SKETCH interrupted by the close request.
                self.state = suspended.map_or(InputState::Command, InputState::Sketch);
                Ok(Effect::Continue)
            }
            InputState::WblockPath => {
                if line.trim().is_empty() {
                    return Err("WBLOCK output file name cannot be empty".into());
                }
                let path = std::path::Path::new(line.trim());
                if path
                    .extension()
                    .is_some_and(|ext| !ext.eq_ignore_ascii_case("dwg"))
                {
                    return Err("WBLOCK output must be a DWG file".into());
                }
                let mut path = path.to_path_buf();
                if path.extension().is_none() {
                    path.set_extension("DWG");
                }
                let path = path.to_string_lossy().into_owned();
                // The question, when the host finds the file, comes before
                // the block name, as in the original.
                self.state = InputState::WblockName(WblockOutput {
                    path: path.clone(),
                    replace: false,
                });
                Ok(Effect::CheckWblockDestination(path))
            }
            InputState::WblockReplace { path, .. } => {
                // Original rule: an answer starting with Y replaces; anything
                // else, including Return, ends WBLOCK keeping the file.
                if line.trim_start().starts_with(['Y', 'y']) {
                    self.state = InputState::WblockName(WblockOutput {
                        path,
                        replace: true,
                    });
                } else {
                    self.status = format!("WBLOCK: kept existing {path}");
                    self.state = InputState::Command;
                }
                Ok(Effect::Continue)
            }
            InputState::WblockName(output) => {
                if line.trim().is_empty() {
                    self.state = InputState::WblockBase(output);
                    return Ok(Effect::Continue);
                }
                let drawing = if line.trim() == "*" {
                    self.wblock_entire_drawing()?
                } else {
                    self.wblock_named_block(line.trim())?
                };
                self.state = InputState::Command;
                Ok(wblock_effect(output, drawing))
            }
            InputState::WblockBase(output) => {
                self.state = InputState::WblockSelection(output, point(line)?);
                Ok(Effect::Continue)
            }
            InputState::WblockSelection(output, base) => {
                let ids = selection(line, selectable_count(&self.drawing))?;
                let drawing = self.wblock_selected_entities(&ids, base)?;
                self.state = InputState::Command;
                Ok(wblock_effect(output, drawing))
            }
            InputState::HelpCommand => {
                let report = help_report(line)?;
                self.status = if line.trim().is_empty() {
                    "Command list".into()
                } else {
                    format!("Help for {}", line.trim().to_ascii_uppercase())
                };
                self.state = InputState::Command;
                Ok(Effect::Report(report))
            }
            InputState::MenuFile => {
                if line.trim().is_empty() {
                    return self.cancel();
                }
                self.state = InputState::Command;
                Ok(Effect::LoadMenu(line.trim().to_owned()))
            }
            InputState::ScriptFile => {
                let name = line.trim();
                if name.is_empty() {
                    return Err("SCRIPT requires a file name".into());
                }
                self.state = InputState::Command;
                Ok(Effect::Script(name.to_owned()))
            }
            InputState::FilesMenu => match line.trim() {
                "0" => self.cancel(),
                "1" => {
                    self.state = InputState::FilesDrive(FilesFilter::Drawings);
                    Ok(Effect::Continue)
                }
                "2" => {
                    self.state = InputState::FilesDrive(FilesFilter::Menus);
                    Ok(Effect::Continue)
                }
                "3" => {
                    self.state = InputState::FilesDrive(FilesFilter::Shapes);
                    Ok(Effect::Continue)
                }
                "4" => {
                    self.state = InputState::FilesDrive(FilesFilter::Patterns);
                    Ok(Effect::Continue)
                }
                "5" => {
                    self.state = InputState::FilesListSpecification;
                    Ok(Effect::Continue)
                }
                "6" => {
                    self.state = InputState::FilesDeleteSpecification;
                    Ok(Effect::Continue)
                }
                "7" => {
                    self.state = InputState::FilesRenameSource;
                    Ok(Effect::Continue)
                }
                _ => Err("FILES selection must be 0 through 7".into()),
            },
            InputState::FilesDrive(filter) => {
                let drive = line.trim();
                if drive.len() != 1 || !drive.as_bytes()[0].is_ascii_alphabetic() {
                    return Err("FILES drive must be a single letter".into());
                }
                self.state = InputState::FilesMenu;
                Ok(Effect::Files(FilesRequest::ListDrive {
                    filter,
                    drive: drive.as_bytes()[0].to_ascii_uppercase() as char,
                }))
            }
            InputState::FilesListSpecification => {
                if line.trim().is_empty() {
                    self.state = InputState::FilesMenu;
                    return Ok(Effect::Continue);
                }
                self.state = InputState::FilesMenu;
                Ok(Effect::Files(FilesRequest::ListSpecification(
                    line.trim().to_owned(),
                )))
            }
            InputState::FilesDeleteSpecification => {
                if line.trim().is_empty() {
                    self.state = InputState::FilesMenu;
                    return Ok(Effect::Continue);
                }
                self.state = InputState::FilesMenu;
                Ok(Effect::Files(FilesRequest::Delete(line.trim().to_owned())))
            }
            InputState::FilesRenameSource => {
                if line.trim().is_empty() {
                    self.state = InputState::FilesMenu;
                    return Ok(Effect::Continue);
                }
                self.state = InputState::FilesRenameDestination(line.trim().to_owned());
                Ok(Effect::Continue)
            }
            InputState::FilesRenameDestination(source) => {
                if line.trim().is_empty() {
                    self.state = InputState::FilesMenu;
                    return Ok(Effect::Continue);
                }
                self.state = InputState::FilesMenu;
                Ok(Effect::Files(FilesRequest::Rename {
                    source,
                    destination: line.trim().to_owned(),
                }))
            }

            _ => unreachable!("dispatch routes only files prompts"),
        }
    }
}

fn wblock_effect(output: WblockOutput, drawing: acad_model::Drawing) -> Effect {
    if output.replace {
        Effect::ReplaceDrawing(output.path, Box::new(drawing))
    } else {
        Effect::SaveDrawing(output.path, Box::new(drawing))
    }
}
