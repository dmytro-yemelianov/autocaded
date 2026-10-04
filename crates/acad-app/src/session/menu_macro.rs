//! Picked screen-menu macros, including `\` pauses (docs/native-files-menu.md).
use super::*;
use acad_cmd::menu::MacroStep;
use std::collections::VecDeque;

impl Session {
    /// Start a picked macro; any macro still paused is abandoned first.
    pub(crate) fn run_macro(
        &mut self,
        text: &str,
        handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) {
        self.paused_macro = None;
        let steps = acad_cmd::menu::macro_steps(text).into();
        self.continue_macro(steps, handle_result);
    }

    /// After one user submission, continue a paused macro.
    pub(crate) fn resume_macro(
        &mut self,
        handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) {
        if let Some(steps) = self.paused_macro.take() {
            self.continue_macro(steps, handle_result);
        }
    }

    fn continue_macro(
        &mut self,
        mut steps: VecDeque<MacroStep>,
        handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) {
        while let Some(step) = steps.pop_front() {
            match step {
                MacroStep::Return(piece) => {
                    let result = if let Some(spec) = self.editor.insert_file_request(&piece) {
                        self.submit_insert_file(&piece, &spec)
                    } else if let Some(spec) = self.editor.hatch_pattern_file_request(&piece) {
                        self.submit_hatch_pattern_file(&piece, &spec)
                    } else {
                        self.editor.submit(&piece)
                    };
                    handle_result(self, result);
                    if self.main_menu.is_some() {
                        // END/QUIT returned to the Main Menu: the rest of
                        // the macro belonged to the closed drawing.
                        return;
                    }
                }
                MacroStep::Pause(typed) => {
                    self.input = typed;
                    self.paused_macro = Some(steps);
                    return;
                }
            }
        }
    }

    /// API/MCP `command`: typed text then Return. At a macro pause the
    /// text appends to the macro's typed prefix, exactly like GUI typing
    /// (`line 1,\7,7;` plus `1` submits `1,1`); otherwise it replaces the
    /// input line as before.
    pub fn command_typed(&mut self, typed: &str) -> Result<bool, String> {
        if self.paused_macro.is_some() {
            let line = format!("{}{typed}", self.input);
            return self.command(&line);
        }
        self.command(typed)
    }

    /// Drop a paused macro (cancel, reset).
    pub(crate) fn abandon_macro(&mut self) {
        self.paused_macro = None;
    }
}
