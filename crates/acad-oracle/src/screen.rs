//! QEMU `screendump` images, scaled to the CGA monitor's 640×400 display.
//! Text-mode screens are rendered correctly by QEMU itself.

use crate::cga::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

pub struct Screen {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

impl Screen {
    /// Parse binary PPM (`P6`, maxval 255), the format QEMU's `screendump`
    /// writes by default.
    pub fn from_ppm(bytes: &[u8]) -> Result<Self, String> {
        let mut fields = Vec::new();
        let mut at = 0;
        while fields.len() < 4 {
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            let start = at;
            while bytes.get(at).is_some_and(|b| !b.is_ascii_whitespace()) {
                at += 1;
            }
            if start == at {
                return Err("truncated PPM header".into());
            }
            fields
                .push(std::str::from_utf8(&bytes[start..at]).map_err(|_| "non-ASCII PPM header")?);
        }
        at += 1; // the single whitespace byte that ends the header
        if fields[0] != "P6" {
            return Err(format!("unsupported PPM magic {:?}", fields[0]));
        }
        let number = |s: &str| {
            s.parse::<usize>()
                .map_err(|_| format!("bad PPM number {s:?}"))
        };
        let (width, height, max) = (number(fields[1])?, number(fields[2])?, number(fields[3])?);
        if max != 255 {
            return Err(format!("unsupported PPM maxval {max}"));
        }
        if width == 0 || height == 0 {
            return Err("empty PPM image".into());
        }
        let len = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(3))
            .ok_or("PPM size overflow")?;
        let rgb = bytes
            .get(at..)
            .and_then(|rest| rest.get(..len))
            .ok_or("truncated PPM pixels")?
            .to_vec();
        Ok(Self { width, height, rgb })
    }

    /// `0x00RRGGBB` pixels, 640×400. A 720-pixel-wide VGA text screen drops
    /// the ninth column of each 9-pixel character cell: CGA cells are 8
    /// pixels wide, and VGA's ninth column only repeats the eighth for
    /// line-drawing glyphs. Other sizes use nearest-neighbour scaling.
    pub fn to_640x400(&self) -> Vec<u32> {
        let column = |x: usize| {
            if self.width == 720 {
                x / 8 * 9 + x % 8
            } else {
                x * self.width / DISPLAY_WIDTH
            }
        };
        let mut out = Vec::with_capacity(DISPLAY_WIDTH * DISPLAY_HEIGHT);
        for y in 0..DISPLAY_HEIGHT {
            let row = y * self.height / DISPLAY_HEIGHT * self.width;
            for x in 0..DISPLAY_WIDTH {
                let i = (row + column(x)) * 3;
                out.push(
                    u32::from(self.rgb[i]) << 16
                        | u32::from(self.rgb[i + 1]) << 8
                        | u32::from(self.rgb[i + 2]),
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ppm(width: usize, height: usize, pixel: impl Fn(usize, usize) -> [u8; 3]) -> Vec<u8> {
        let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
        for y in 0..height {
            for x in 0..width {
                bytes.extend(pixel(x, y));
            }
        }
        bytes
    }

    #[test]
    fn parses_qemu_ppm() {
        let screen = Screen::from_ppm(&ppm(2, 1, |x, _| [x as u8, 2, 3])).unwrap();
        assert_eq!((screen.width, screen.height), (2, 1));
        assert_eq!(screen.rgb, [0, 2, 3, 1, 2, 3]);
    }

    #[test]
    fn rejects_malformed_ppm() {
        assert!(Screen::from_ppm(b"P5\n1 1\n255\n\0").is_err());
        assert!(Screen::from_ppm(b"P6\n1 1\n").is_err());
        assert!(Screen::from_ppm(b"P6\n2 2\n255\n\0\0\0").is_err());
        assert!(Screen::from_ppm(b"P6\n1 1\n65535\n\0\0\0\0\0\0").is_err());
        assert!(Screen::from_ppm(b"P6\n0 0\n255\n").is_err());
    }

    #[test]
    fn vga_text_drops_ninth_cell_column() {
        let screen =
            Screen::from_ppm(&ppm(720, 400, |x, y| [(x % 9) as u8 * 10, y as u8, 0])).unwrap();
        let out = screen.to_640x400();
        assert_eq!(out.len(), DISPLAY_WIDTH * DISPLAY_HEIGHT);
        for (x, pixel) in out.iter().take(DISPLAY_WIDTH).enumerate() {
            assert_eq!(*pixel >> 16, (x % 8) as u32 * 10, "column {x}");
        }
        assert_eq!((out[DISPLAY_WIDTH * 399] >> 8) & 0xFF, 399 % 256);
    }

    #[test]
    fn other_sizes_scale_without_panicking() {
        for (w, h) in [(640, 480), (1, 1), (360, 400), (1024, 768)] {
            let screen = Screen::from_ppm(&ppm(w, h, |_, _| [9, 8, 7])).unwrap();
            let out = screen.to_640x400();
            assert_eq!(out.len(), DISPLAY_WIDTH * DISPLAY_HEIGHT);
            assert!(out.iter().all(|&p| p == 0x09_0807), "{w}x{h}");
        }
    }
}
