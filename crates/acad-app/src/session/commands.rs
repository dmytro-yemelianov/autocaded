use super::*;

impl Session {
    pub(crate) fn submit_return_input(
        &mut self,
        handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) {
        let input = std::mem::take(&mut self.input);
        self.clear_status();
        let command_input = if self.editor.awaiting_shape_library_name() {
            match self.resolve_shape_library(&input) {
                Ok(name) => name,
                Err(error) => {
                    self.set_status(error);
                    return;
                }
            }
        } else if let Some(spec) = self.editor.insert_file_request(&input) {
            let result = self.submit_insert_file(&input, &spec);
            handle_result(self, result);
            self.resume_macro(handle_result);
            return;
        } else if let Some(spec) = self.editor.hatch_pattern_file_request(&input) {
            let result = self.submit_hatch_pattern_file(&input, &spec);
            handle_result(self, result);
            self.resume_macro(handle_result);
            return;
        } else {
            input
        };
        let asked = self.script_stepping && self.editor.asking_original_erasure();
        let result = self.editor.submit_return(&command_input);
        self.capture_command_diagnostic(&result);
        let result = self.automated_erasure_answer(asked, result);
        handle_result(self, result);
        self.resume_macro(handle_result);
    }
}
