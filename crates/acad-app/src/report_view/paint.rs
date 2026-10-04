//! Report canvas and mouse paging controls, above the persistent command area.
use super::{layout::Layout, Report};
use crate::{
    bitmap::{draw_rect, draw_text, GLYPH_HEIGHT, GLYPH_WIDTH},
    command_line,
};
const PAD: usize = 4;
const CELL: usize = GLYPH_WIDTH * 2;
const ROW: usize = GLYPH_HEIGHT * 2;
const BAR: usize = ROW + PAD;
const BACKGROUND: u32 = 0x0010_1820;
const TEXT: u32 = 0x00dd_eeee;
const CONTROL: u32 = 0x00ff_c080;

pub(super) fn columns(width: u32) -> usize {
    (width as usize)
        .saturating_sub(2 * PAD)
        .div_euclid(CELL)
        .max(1)
}

pub(super) fn rows(height: u32) -> usize {
    (command_line::drawing_height(height) as usize).saturating_sub(2 * BAR) / ROW
}
pub(super) fn footer_contains(y: f64, height: u32) -> bool {
    let canvas = command_line::drawing_height(height) as usize;
    canvas >= 2 * BAR + ROW && y >= (canvas - BAR) as f64 && y < canvas as f64
}
pub(super) fn draw(report: &Report, layout: &Layout, buffer: &mut [u32], width: u32, height: u32) {
    let count = rows(height);
    let canvas = command_line::drawing_height(height) as usize;
    let width = width as usize;
    let length = (width * canvas).min(buffer.len());
    let buffer = &mut buffer[..length];
    draw_rect(buffer, width, canvas, 0, 0, width, canvas, BACKGROUND);
    if count == 0 {
        return;
    }
    let top = layout.top(report.anchor, count);
    let title = format!(
        "REPORT {}-{} / {}",
        top + 1,
        (top + count).min(layout.lines.len()),
        layout.lines.len()
    );
    let title: String = title.chars().take(columns(width as u32)).collect();
    draw_text(buffer, width, PAD, PAD, &title, CONTROL);
    for (row, line) in layout.lines.iter().skip(top).take(count).enumerate() {
        draw_text(
            buffer,
            width,
            PAD,
            BAR + row * ROW,
            &report.display[line.clone()],
            TEXT,
        );
    }
    for (index, label) in if width < 360 {
        ["UP", "DN", "X"]
    } else {
        ["PREV", "NEXT", "CLOSE"]
    }
    .iter()
    .enumerate()
    {
        let left = width * index / 3;
        let right = width * (index + 1) / 3;
        let cells = (right - left).saturating_sub(2 * PAD) / CELL;
        let label: String = label.chars().take(cells).collect();
        draw_text(
            buffer,
            width,
            left + PAD,
            canvas - BAR + PAD,
            &label,
            CONTROL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn end_paints_the_last_source_line_and_never_touches_the_command_area() {
        let mut report = Report::new("FIRST\nSECOND\nTHIRD\nTAIL".into());
        let (width, height) = (160, 100); // exactly one report body row
        report.navigate(crate::ReportAction::End, width, height);
        let sentinel = 0x0012_3456;
        let mut actual = vec![sentinel; (width * height) as usize];
        report.draw(&mut actual, width, height);
        let mut expected = vec![BACKGROUND; (width * height) as usize];
        draw_text(&mut expected, width as usize, PAD, BAR, "TAIL", TEXT);
        let body = BAR * width as usize..(BAR + ROW) * width as usize;
        assert!(actual[body.clone()] == expected[body]);
        let canvas = command_line::drawing_height(height) as usize;
        assert!(actual[canvas * width as usize..]
            .iter()
            .all(|&p| p == sentinel));
        assert_eq!(report.anchor(), "FIRST\nSECOND\nTHIRD\n".len());
    }
}
