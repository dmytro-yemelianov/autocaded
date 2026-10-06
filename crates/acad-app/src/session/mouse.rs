use super::*;

impl Session {
    /// Native navigation gesture: move ink with the pointer, using the
    /// rendered canvas scale and the ordinary PAN/Previous command contract.
    /// Pending command input, reports and the Main Menu are left untouched.
    pub fn pan_pixels(
        &mut self,
        dx: f64,
        dy: f64,
        width: u32,
        height: u32,
    ) -> Result<bool, String> {
        if !dx.is_finite() || !dy.is_finite() {
            return Err("pan displacement must be finite".into());
        }
        if !self.command_idle() || !self.input.is_empty() || self.report_visible() {
            return Ok(false);
        }
        self.set_viewport_size(width, height)?;
        if dx == 0.0 && dy == 0.0 {
            return Ok(false);
        }
        let view = self.drawing().header.view;
        let canvas_height = command_line::drawing_height(height).max(1);
        let aspect = f64::from(width) / f64::from(canvas_height);
        view.validate_canvas(aspect, canvas_height)
            .map_err(str::to_owned)?;
        let units_per_pixel = view.height / f64::from(canvas_height);
        let delta = acad_model::Point {
            x: -dx * units_per_pixel,
            y: dy * units_per_pixel,
        };
        let candidate = acad_model::DwgView {
            center: acad_model::Point {
                x: view.center.x + delta.x,
                y: view.center.y + delta.y,
            },
            height: view.height,
        };
        candidate
            .validate_canvas(aspect, canvas_height)
            .map_err(str::to_owned)?;
        self.command("PAN")?;
        self.command(&format!("@{},{}", delta.x, delta.y))?;
        self.command("")
    }

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
                    self.set_status(format!(
                        "{} is not yet implemented (control byte 0x{:02x})",
                        entry.label, entry.action[0]
                    ));
                    return true;
                }
                let Ok(text) = std::str::from_utf8(&entry.action) else {
                    self.set_status(format!("{}: macro is not valid UTF-8", entry.label));
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
        self.clear_status();
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
                self.set_status(format!("Selected entity {id}"));
            }
            Ok(None) => self.set_status("No entity at pick point".into()),
            Err(error) => self.set_status(error),
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
    /// GUI left-button press. In SKETCH over the drawing it starts a drag
    /// (native policy, docs/native-sketch.md): the press lowers the pen there,
    /// or connects when the press is within the increment of the last end
    /// point, and [`Session::release`] lifts it. Erase confirmation, the
    /// screen menu and every other prompt treat the press as a click.
    pub fn press(&mut self, x: f64, y: f64, width: u32, height: u32) -> Result<bool, String> {
        self.sketch_drag = false;
        if self.editor.sketch_active() && !self.report_visible() {
            if !x.is_finite() || !y.is_finite() {
                return Err("press must be inside the frame".into());
            }
            self.set_viewport_size(width, height)?;
            self.cursor = Some((x, y));
            if self.drawing_cursor_world(width, height).is_some() {
                let before = self.editor.sketch_preview().expect("SKETCH is active");
                if before.mode != acad_cmd::SketchMode::Erase {
                    self.pointer_motion(width, height)?;
                    let after = self.editor.sketch_preview().expect("SKETCH is active");
                    // A connect within the increment has lowered the pen.
                    if after.pen_down {
                        self.sketch_drag = true;
                        return Ok(false);
                    }
                    if before.mode == acad_cmd::SketchMode::Draw {
                        let quit = self.command("P")?;
                        self.sketch_drag = self.editor.sketch_preview().is_some_and(|s| s.pen_down);
                        return Ok(quit);
                    }
                }
            }
        }
        self.click(x, y, width, height)
    }

    /// GUI left-button release: ends a SKETCH drag by lifting the pen, which
    /// records the tail to the last pointer inside the drawing. A release
    /// outside the frame (or with no position) lifts at that last pointer.
    pub fn release(&mut self, x: f64, y: f64, width: u32, height: u32) -> Result<bool, String> {
        if !std::mem::take(&mut self.sketch_drag) {
            return Ok(false);
        }
        let Some(sketch) = self.editor.sketch_preview() else {
            return Ok(false);
        };
        if !sketch.pen_down || sketch.mode != acad_cmd::SketchMode::Draw {
            return Ok(false);
        }
        if x.is_finite() && y.is_finite() {
            self.cursor = Some((x, y));
            self.pointer_motion(width, height)?;
        }
        self.command("P")
    }

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
