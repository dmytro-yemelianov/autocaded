use super::*;

impl Session {
    /// Dispatches a left-click against the rendered screen-menu panel, if
    /// one is loaded and the click lands inside it. Returns `true` when the
    /// click was handled (including an unsupported
    /// control-byte entry or a blank panel slot) so the caller skips the ordinary
    /// `submit_mouse_point`/`pick_mouse_entity` dispatch; returns `false`
    /// when there is no menu, no cursor position, or the click fell outside
    /// the panel rect, so the caller should fall through as usual.
    ///
    /// Picked text macros run through [`Session::run_macro`]: every space
    /// and `;` is one Return and `\` pauses for one user input, as measured
    /// on the original (docs/native-files-menu.md). Blank slots are inert.
    pub(crate) fn handle_panel_click(
        &mut self,
        width: u32,
        height: u32,
        handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) -> bool {
        let Some(menu) = self.menu.clone() else {
            return false;
        };
        let Some((x, y)) = self.cursor else {
            return false;
        };
        let layout = menu_panel::layout_for(
            &menu,
            self.menu_page,
            width,
            command_line::drawing_height(height),
        );
        if !layout.contains(x, y) {
            return false;
        }
        let Some(hit) = menu_panel::entry_at(&menu, self.menu_page, &layout, x, y) else {
            return true;
        };
        match hit {
            menu_panel::PanelHit::Entry(index) => {
                let Some(entry) = menu_panel::resolve_entry(&menu, self.menu_page, index) else {
                    return true;
                };
                if entry.kind == acad_cmd::menu::MenuEntryKind::Blank {
                    return true;
                }
                let control = match entry.action.as_slice() {
                    [0x02] => Some(acad_cmd::MenuControl::Snap),
                    [0x0f] => Some(acad_cmd::MenuControl::Ortho),
                    [0x03] => Some(acad_cmd::MenuControl::Cancel),
                    _ => None,
                };
                if let Some(control) = control {
                    if control == acad_cmd::MenuControl::Cancel {
                        self.input.clear();
                        self.abandon_macro();
                    }
                    let result = self.editor.apply_menu_control(control);
                    handle_result(self, result);
                    return true;
                }
                if entry.action.len() == 1 && entry.action[0] < 0x20 {
                    self.status = format!(
                        "{} is not yet implemented (control byte 0x{:02x})",
                        entry.label, entry.action[0]
                    );
                    return true;
                }
                let Ok(text) = std::str::from_utf8(&entry.action) else {
                    self.status = format!("{}: macro is not valid UTF-8", entry.label);
                    return true;
                };
                let text = text.to_owned();
                self.run_macro(&text, handle_result);
                true
            }
            menu_panel::PanelHit::Next => {
                self.menu_page = advance_menu_page(self.menu_page, &menu);
                true
            }
            menu_panel::PanelHit::Go => {
                self.submit_return_input(handle_result);
                true
            }
        }
    }

    /// The real MouseInput routing seam. Only effect handling needs an event
    /// loop; tests can observe effects while exercising this entire route.
    pub(crate) fn handle_left_click(
        &mut self,
        width: u32,
        height: u32,
        mut handle_result: impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) {
        if let Err(error) = self.set_viewport_size(width, height) {
            handle_result(self, Err(error));
            return;
        }
        if self
            .cursor
            .is_some_and(|(x, y)| command_line::contains(width, height, x, y))
        {
            return;
        }
        if self.report_visible() {
            if let (Some(report), Some((x, y))) = (&mut self.report, self.cursor) {
                report.click(x, y, width, height);
            }
            return;
        }
        if !self.handle_panel_click(width, height, &mut handle_result) {
            if self.editor.accepts_mouse_point() {
                self.input.clear();
                self.submit_mouse_point(width, height, &mut handle_result);
            } else {
                self.pick_mouse_entity(width, height);
            }
        }
    }

    pub(crate) fn submit_mouse_point(
        &mut self,
        width: u32,
        height: u32,
        handle_result: &mut impl FnMut(&mut Self, Result<acad_cmd::Effect, String>),
    ) {
        if !self.editor.accepts_mouse_point() {
            return;
        }
        let Some((x, y)) = self.cursor else {
            return;
        };
        let vp = viewport_for(self.editor.drawing(), width, height);
        let world = vp.to_world(acad_model::Point { x, y });
        self.status.clear();
        let result = self.editor.submit_mouse_point(world);
        handle_result(self, result);
        self.resume_macro(handle_result);
    }

    pub(crate) fn pick_mouse_entity(&mut self, width: u32, height: u32) {
        if !self.editor.accepts_mouse_selection() {
            return;
        }
        let Some((x, y)) = self.cursor else {
            return;
        };
        let vp = viewport_for(self.editor.drawing(), width, height);
        let world = vp.to_world(acad_model::Point { x, y });
        let horizontal_neighbor = vp.to_world(acad_model::Point { x: x + 6.0, y });
        let tolerance = (horizontal_neighbor.x - world.x).hypot(horizontal_neighbor.y - world.y);
        match self.editor.pick_selection_at(world, tolerance) {
            Ok(Some(id)) => {
                self.input = self
                    .editor
                    .collected_selection()
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                self.status = format!("Selected entity {id}");
            }
            Ok(None) => self.status = "No entity at pick point".into(),
            Err(error) => self.status = error,
        }
    }

    /// World position of the stored cursor when it is over the drawing area,
    /// excluding the screen-menu panel and the command area.
    fn drawing_cursor_world(&self, width: u32, height: u32) -> Option<acad_model::Point> {
        let (x, y) = self.cursor?;
        let canvas_height = command_line::drawing_height(height);
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= f64::from(width)
            || y >= f64::from(canvas_height)
        {
            return None;
        }
        if let Some(menu) = &self.menu {
            if menu_panel::layout_for(menu, self.menu_page, width, canvas_height).contains(x, y) {
                return None;
            }
        }
        Some(
            viewport_for(self.editor.drawing(), width, height).to_world(acad_model::Point { x, y }),
        )
    }

    /// Pointer-motion seam shared by the GUI `CursorMoved` handler and the
    /// API `motion` request: SKETCH samples the stored cursor's world point.
    /// Motion outside the drawing area, or outside SKETCH, changes nothing.
    pub fn pointer_motion(&mut self, width: u32, height: u32) -> Result<bool, String> {
        if !self.editor.sketch_active() || self.report_visible() {
            return Ok(false);
        }
        self.set_viewport_size(width, height)?;
        let Some(world) = self.drawing_cursor_world(width, height) else {
            return Ok(false);
        };
        let result = self.editor.pointer_moved(world);
        self.apply_effect(result)
    }

    /// Preview the same world point that a drawing click will submit. Menu and
    /// command-area hits use the physical cursor; they never preview drawing input.
    pub(crate) fn crosshair_position(&self, width: u32, height: u32) -> Option<(f64, f64)> {
        let (x, y) = self.cursor?;
        let canvas_height = command_line::drawing_height(height);
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= f64::from(width)
            || y >= f64::from(canvas_height)
        {
            return None;
        }
        if let Some(menu) = &self.menu {
            if menu_panel::layout_for(menu, self.menu_page, width, canvas_height).contains(x, y) {
                return None;
            }
        }
        if !self.editor.accepts_mouse_point() {
            return Some((x, y));
        }
        let viewport = viewport_for(self.editor.drawing(), width, height);
        let world = viewport.to_world(acad_model::Point { x, y });
        let constrained = self.editor.constrain_mouse_point(world).ok()?;
        if constrained == world {
            return Some((x, y));
        }
        let screen = viewport.to_screen(constrained);
        Some((screen.x, screen.y))
    }
}
