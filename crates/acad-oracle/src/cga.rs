//! AutoCAD's CGA screen, reconstructed from a 16 KiB dump of `B800:0000`.
//!
//! The original's display driver programs the CGA mode register directly,
//! which QEMU's VGA ignores, so the adapter's own picture is useless. Video
//! memory is still exactly what a CGA would scan out.

pub const VIDEO_BYTES: usize = 16384;
pub const DISPLAY_WIDTH: usize = 640;
pub const DISPLAY_HEIGHT: usize = 400;
const BANK: usize = 0x2000;
const ROW_BYTES: usize = 80;
const TEXT_CELLS: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Text,
    Graphics,
}

/// The 640×200 two-colour bitmap. Even scanlines start at offset 0, odd
/// scanlines at `0x2000`; the high bit of each byte is the leftmost pixel.
pub struct Frame<'a>(&'a [u8]);

impl<'a> Frame<'a> {
    pub fn new(memory: &'a [u8]) -> Result<Self, String> {
        if memory.len() != VIDEO_BYTES {
            return Err(format!(
                "CGA memory is {} bytes, expected {VIDEO_BYTES}",
                memory.len()
            ));
        }
        Ok(Self(memory))
    }

    pub fn lit(&self, x: usize, y: usize) -> bool {
        self.0[(y % 2) * BANK + (y / 2) * ROW_BYTES + x / 8] & (0x80 >> (x % 8)) != 0
    }

    /// White-on-black `0x00RRGGBB` pixels with each scanline shown twice,
    /// matching the 640×400 text screenshot size.
    pub fn to_rgb_640x400(&self) -> Vec<u32> {
        (0..DISPLAY_HEIGHT)
            .flat_map(|y| (0..DISPLAY_WIDTH).map(move |x| (x, y / 2)))
            .map(|(x, y)| if self.lit(x, y) { 0xFF_FFFF } else { 0 })
            .collect()
    }
}

/// Classify a dump without seeing the mode register. An 80×25 text page
/// stores character/attribute pairs, and DOS writes nearly every attribute
/// with the same nonzero value. Blank bitmaps are zero; drawn ones vary; a
/// solid fill repeats one value in the character bytes as well.
pub fn detect(memory: &[u8]) -> Option<Mode> {
    let page = memory.get(..TEXT_CELLS * 2)?;
    let mut counts = [0usize; 256];
    for pair in page.chunks_exact(2) {
        counts[usize::from(pair[1])] += 1;
    }
    let (value, &count) = counts
        .iter()
        .enumerate()
        .max_by_key(|&(_, count)| *count)
        .expect("256 counters");
    let same_chars = page
        .chunks_exact(2)
        .filter(|pair| usize::from(pair[0]) == value)
        .count();
    let dominant = |n: usize| n * 100 >= TEXT_CELLS * 95;
    let text = value != 0 && dominant(count) && !dominant(same_chars);
    Some(if text { Mode::Text } else { Mode::Graphics })
}

/// Mode with hysteresis: a change needs two consecutive agreeing frames, so a
/// partially redrawn screen does not flicker. Undecidable frames are ignored.
pub struct ModeTracker {
    current: Mode,
    pending: Option<Mode>,
}

impl ModeTracker {
    /// DOS boots in text mode.
    pub fn new() -> Self {
        Self {
            current: Mode::Text,
            pending: None,
        }
    }

    pub fn update(&mut self, memory: &[u8]) -> Mode {
        match detect(memory) {
            None => {}
            Some(mode) if mode == self.current => self.pending = None,
            Some(mode) if self.pending == Some(mode) => {
                self.current = mode;
                self.pending = None;
            }
            Some(mode) => self.pending = Some(mode),
        }
        self.current
    }
}

impl Default for ModeTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_page(chars: impl Fn(usize) -> u8, attribute: u8) -> Vec<u8> {
        let mut memory = vec![0; VIDEO_BYTES];
        for cell in 0..2000 {
            memory[cell * 2] = chars(cell);
            memory[cell * 2 + 1] = attribute;
        }
        memory
    }

    #[test]
    fn scanlines_interleave_and_high_bit_is_leftmost() {
        let mut memory = vec![0; VIDEO_BYTES];
        memory[0] = 0x80; // (0, 0)
        memory[0x2000] = 0x01; // (7, 1)
        memory[80] = 0x40; // (1, 2)
        memory[0x2000 + 99 * 80 + 79] = 0x01; // (639, 199)
        let frame = Frame::new(&memory).unwrap();
        assert!(frame.lit(0, 0) && frame.lit(7, 1) && frame.lit(1, 2) && frame.lit(639, 199));
        let total = (0..200)
            .flat_map(|y| (0..640).map(move |x| (x, y)))
            .filter(|&(x, y)| frame.lit(x, y))
            .count();
        assert_eq!(total, 4);
    }

    #[test]
    fn frame_rejects_wrong_length() {
        assert!(Frame::new(&[0; 4000]).is_err());
    }

    #[test]
    fn display_doubles_each_scanline() {
        let mut memory = vec![0; VIDEO_BYTES];
        memory[0] = 0x80;
        let rgb = Frame::new(&memory).unwrap().to_rgb_640x400();
        assert_eq!(rgb.len(), DISPLAY_WIDTH * DISPLAY_HEIGHT);
        assert_eq!(
            (rgb[0], rgb[640], rgb[1280], rgb[1]),
            (0xFF_FFFF, 0xFF_FFFF, 0, 0)
        );
    }

    #[test]
    fn blank_dos_screen_is_text() {
        assert_eq!(detect(&text_page(|_| b' ', 0x07)), Some(Mode::Text));
    }

    #[test]
    fn dense_text_is_text() {
        let page = text_page(|cell| (cell % 95) as u8 + 32, 0x07);
        assert_eq!(detect(&page), Some(Mode::Text));
    }

    #[test]
    fn blank_bitmap_is_graphics() {
        assert_eq!(detect(&vec![0; VIDEO_BYTES]), Some(Mode::Graphics));
    }

    #[test]
    fn busy_bitmap_is_graphics() {
        let mut seed = 12345u32;
        let memory: Vec<u8> = (0..VIDEO_BYTES)
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                (seed >> 16) as u8
            })
            .collect();
        assert_eq!(detect(&memory), Some(Mode::Graphics));
    }

    #[test]
    fn solid_fill_is_graphics() {
        assert_eq!(detect(&vec![0xFF; VIDEO_BYTES]), Some(Mode::Graphics));
    }

    #[test]
    fn short_memory_is_undecided() {
        assert_eq!(detect(&[0x20, 0x07]), None);
    }

    #[test]
    fn tracker_needs_two_agreeing_frames() {
        let text = text_page(|_| b' ', 0x07);
        let graphics = vec![0; VIDEO_BYTES];
        let mut tracker = ModeTracker::new();
        assert_eq!(tracker.update(&graphics), Mode::Text);
        assert_eq!(tracker.update(&graphics), Mode::Graphics);
        assert_eq!(tracker.update(&text), Mode::Graphics);
        assert_eq!(tracker.update(&graphics), Mode::Graphics);
        assert_eq!(tracker.update(&text), Mode::Graphics);
        assert_eq!(tracker.update(&[0]), Mode::Graphics);
        assert_eq!(tracker.update(&text), Mode::Text);
    }
}
