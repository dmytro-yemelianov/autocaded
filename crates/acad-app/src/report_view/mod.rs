//! Read-only report presentation; editor prompts and drawing data stay separate.
mod layout;
mod paint;
use layout::Layout;
use serde::Deserialize;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportAction {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Close,
    Open,
}

pub(crate) struct Report {
    text: String,
    display: String,
    visible: bool,
    anchor: usize,
    layout: RefCell<Layout>,
}

impl Report {
    pub(crate) fn new(text: String) -> Self {
        let mut display = String::new();
        let mut column = 0;
        for ch in text.chars() {
            match ch {
                '\r' => continue,
                '\n' => {
                    display.push('\n');
                    column = 0;
                }
                '\t' => {
                    let spaces = 4 - column % 4;
                    display.extend(std::iter::repeat_n(' ', spaces));
                    column += spaces;
                }
                ch => {
                    display.push(if ch.is_ascii() && !ch.is_control() {
                        ch
                    } else {
                        '?'
                    });
                    column += 1;
                }
            }
        }
        Self {
            text,
            display,
            visible: true,
            anchor: 0,
            layout: RefCell::new(Layout::default()),
        }
    }
    pub(crate) fn text(&self) -> &str {
        &self.text
    }
    pub(crate) fn visible(&self) -> bool {
        self.visible
    }
    pub(crate) fn anchor(&self) -> usize {
        self.anchor
    }
    pub(crate) fn hide(&mut self) {
        self.visible = false;
    }
    fn with_layout<T>(&self, width: u32, f: impl FnOnce(&Layout) -> T) -> T {
        let mut layout = self.layout.borrow_mut();
        layout.update(&self.display, paint::columns(width));
        f(&layout)
    }
    pub(crate) fn navigate(&mut self, action: ReportAction, width: u32, height: u32) {
        match action {
            ReportAction::Close => {
                self.hide();
                return;
            }
            ReportAction::Open => {
                self.visible = true;
                return;
            }
            _ => {}
        }
        let rows = paint::rows(height).max(1);
        self.anchor = self.with_layout(width, |layout| {
            let top = layout.top(self.anchor, rows);
            let last = layout.lines.len().saturating_sub(rows);
            let target = match action {
                ReportAction::Up => top.saturating_sub(1),
                ReportAction::Down => top.saturating_add(1).min(last),
                ReportAction::PageUp => top.saturating_sub(rows),
                ReportAction::PageDown => top.saturating_add(rows).min(last),
                ReportAction::Home => 0,
                ReportAction::End => last,
                _ => unreachable!(),
            };
            layout.lines[target].start
        });
    }
    pub(crate) fn draw(&self, buffer: &mut [u32], width: u32, height: u32) {
        if self.visible {
            self.with_layout(width, |layout| {
                paint::draw(self, layout, buffer, width, height)
            });
        }
    }
    pub(crate) fn click(&mut self, x: f64, y: f64, width: u32, height: u32) {
        if paint::footer_contains(y, height) && width > 0 {
            let action = match (x * 3.0 / f64::from(width)) as u32 {
                0 => ReportAction::PageUp,
                1 => ReportAction::PageDown,
                _ => ReportAction::Close,
            };
            self.navigate(action, width, height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_normalization_preserves_the_original_api_text() {
        let original = "\tA☃\r\n\nTAIL\u{1b}";
        let report = Report::new(original.into());
        assert_eq!(report.text(), original);
        assert_eq!(report.display, "    A?\n\nTAIL?");
    }
    #[test]
    fn resized_frames_do_not_change_the_navigation_anchor() {
        let mut report = Report::new("long report line\n".repeat(80));
        report.navigate(ReportAction::PageDown, 640, 240);
        let anchor = report.anchor();
        assert!(anchor > 0);
        for (width, height) in [(160, 100), (800, 600)] {
            let mut pixels = vec![0; width * height];
            report.draw(&mut pixels, width as u32, height as u32);
            assert_eq!(report.anchor(), anchor);
        }
        report.navigate(ReportAction::Close, 640, 240);
        report.navigate(ReportAction::Open, 640, 240);
        assert_eq!(report.anchor(), anchor);
    }
}
