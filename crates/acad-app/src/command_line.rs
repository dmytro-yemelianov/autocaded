//! The persistent two-line command area, in physical client pixels.
use crate::menu_panel::{draw_rect, draw_text, GLYPH_HEIGHT, GLYPH_WIDTH};

const CELL: usize = GLYPH_WIDTH * 2;
const ROW: usize = GLYPH_HEIGHT * 2;
const PAD: usize = 4;
pub(crate) const HEIGHT: u32 = (2 * ROW + 3 * PAD) as u32;

pub(crate) fn drawing_height(height: u32) -> u32 {
    height.saturating_sub(HEIGHT)
}

pub(crate) fn contains(width: u32, height: u32, x: f64, y: f64) -> bool {
    x >= 0.0 && x < width as f64 && y >= drawing_height(height) as f64 && y < height as f64
}

/// Keep the prompt when it fits, but prioritize the input's tail and caret
/// when the combined line exceeds the client width. Truncate on character
/// boundaries to match the bitmap renderer's character-cell layout.
fn visible_input(prompt: &str, input: &str, cells: usize) -> String {
    let text = format!("{prompt}: {input}_");
    text.chars()
        .skip(text.chars().count().saturating_sub(cells))
        .collect()
}

pub(crate) fn draw(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    prompt: &str,
    input: &str,
    status: &str,
) {
    let top = drawing_height(height) as usize;
    let width = width as usize;
    let height = height as usize;
    const BACKGROUND: u32 = 0x0018_1820;
    draw_rect(
        buffer,
        width,
        height,
        0,
        top,
        width,
        height - top,
        BACKGROUND,
    );
    draw_rect(buffer, width, height, 0, top, width, 1, 0x0080_8080);
    let cells = width.saturating_sub(2 * PAD) / CELL;
    let input = visible_input(prompt, input, cells);
    let status: String = status.chars().take(cells).collect();
    draw_text(buffer, width, PAD, top + PAD, &input, 0x00ff_ffff);
    // The shared bitmap font has no underscore glyph. Its reserved cell
    // instead gets an explicit caret, without duplicating or changing the font.
    if let Some(last_cell) = input.chars().count().checked_sub(1) {
        draw_rect(
            buffer,
            width,
            height,
            PAD + last_cell * CELL,
            top + PAD + ROW - 2,
            CELL - 4,
            2,
            0x00ff_ffff,
        );
    }
    draw_text(
        buffer,
        width,
        PAD,
        top + ROW + 2 * PAD,
        &status,
        0x00ff_c080,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paints_prompt_typing_and_latest_status_in_separate_rows() {
        let (width, height) = (640, 100);
        let mut actual = vec![0; width * height];
        draw(
            &mut actual,
            width as u32,
            height as u32,
            "Command",
            "LINE",
            "Unknown command",
        );
        let top = drawing_height(height as u32) as usize;
        assert!(actual[..top * width].iter().all(|&pixel| pixel == 0));
        // Compare the glyph regions to the shared font, not merely a nonempty background.
        let mut expected = vec![0; width * height];
        draw_text(
            &mut expected,
            width,
            PAD,
            top + PAD,
            "Command: LINE_",
            0x00ff_ffff,
        );
        draw_text(
            &mut expected,
            width,
            PAD,
            top + ROW + 2 * PAD,
            "Unknown command",
            0x00ff_c080,
        );
        for (actual, expected) in actual.iter().zip(&expected) {
            if *expected != 0 {
                assert_eq!(actual, expected);
            }
        }
        assert!(actual.contains(&0x00ff_c080));
        let caret_x = PAD + "Command: LINE".chars().count() * CELL;
        assert_eq!(actual[(top + PAD + ROW - 2) * width + caret_x], 0x00ff_ffff);
    }

    #[test]
    fn long_unicode_input_keeps_its_end_and_caret_without_panicking() {
        assert_eq!(visible_input("Command", "prefixéTAIL", 5), "TAIL_");
        assert_eq!(visible_input("Command", "abc", 0), "");
        let mut clipped = vec![0; 88 * HEIGHT as usize];
        draw(&mut clipped, 88, HEIGHT, "Command", "prefixéTAIL", "");
        assert_eq!(clipped[(PAD + ROW - 2) * 88 + PAD + 4 * CELL], 0x00ff_ffff);
        for (width, height) in [(0, 0), (1, 1), (20, 8), (80, 44)] {
            let mut pixels = vec![0; width * height];
            draw(
                &mut pixels,
                width as u32,
                height as u32,
                "Command",
                "é".repeat(100).as_str(),
                "error",
            );
        }
    }
}
