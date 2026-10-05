//! AutoCAD Color Index (ACI) palette and RGB conversions.
//!
//! AutoCAD 1.4 defines 256 indexed colors (0..=255):
//! - Color 0 is BYBLOCK (rendered as white when background is black).
//! - Colors 1..=7 are the standard primary colors (Red, Yellow, Green, Cyan, Blue, Magenta, White).
//! - Colors 8..=9 are gray shades.
//! - Colors 10..=249 form 24 hues in 15-degree steps across 5 brightness levels and 2 saturations.
//! - Colors 250..=255 form a standard grayscale ramp from black to white.

pub const COLOR_BYBLOCK: u8 = 0;
pub const COLOR_RED: u8 = 1;
pub const COLOR_YELLOW: u8 = 2;
pub const COLOR_GREEN: u8 = 3;
pub const COLOR_CYAN: u8 = 4;
pub const COLOR_BLUE: u8 = 5;
pub const COLOR_MAGENTA: u8 = 6;
pub const COLOR_WHITE: u8 = 7;

/// How colour numbers become RGB. Only 1..=7 are documented for 1983; what
/// 8 and above looked like depended on the display driver.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Palette {
    /// 16-colour PC display: 1..=7 as documented, 8 dark grey, 9..=14 the
    /// bright variants of 1..=6, 15 bright white (AutoCAD 1.4's default layer
    /// colour). 16 and above fall back to [`aci_rgb`].
    #[default]
    Pc16,
    /// The modern 256-colour ACI table ([`aci_rgb`]) for every index.
    Aci256,
}

impl Palette {
    pub const ALL: [Palette; 2] = [Palette::Pc16, Palette::Aci256];

    pub fn rgb(self, index: u8) -> [u8; 3] {
        match (self, index) {
            (Palette::Pc16, 8) => [85, 85, 85],
            (Palette::Pc16, 9) => [255, 85, 85],
            (Palette::Pc16, 10) => [255, 255, 85],
            (Palette::Pc16, 11) => [85, 255, 85],
            (Palette::Pc16, 12) => [85, 255, 255],
            (Palette::Pc16, 13) => [85, 85, 255],
            (Palette::Pc16, 14) => [255, 85, 255],
            (Palette::Pc16, 15) => [255, 255, 255],
            _ => aci_rgb(index),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Palette::Pc16 => "pc16",
            Palette::Aci256 => "aci256",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|palette| palette.name().eq_ignore_ascii_case(name))
    }
}

/// Convert an AutoCAD Color Index (ACI, 0..=255) into standard [r, g, b] bytes.
pub fn aci_rgb(index: u8) -> [u8; 3] {
    match index {
        // Color 0 is BYBLOCK; use white until block color inheritance is
        // modeled. Index 7 switches between white and black by background;
        // this renderer uses a black background.
        0 | 7 => [255, 255, 255],
        1 => [255, 0, 0],
        2 => [255, 255, 0],
        3 => [0, 255, 0],
        4 => [0, 255, 255],
        5 => [0, 0, 255],
        6 => [255, 0, 255],
        8 => [128, 128, 128],
        9 => [192, 192, 192],
        10..=249 => {
            // ACI 10..249 is 24 hues in 15-degree steps. Each hue has five
            // brightness levels, each in full and half saturation.
            let color = index - 10;
            let hue = f64::from(color / 10) * 15.0;
            let value = [255.0, 165.0, 127.0, 76.0, 38.0][usize::from((color % 10) / 2)];
            let saturation = if color % 2 == 0 { 255.0 } else { 127.0 };
            let chroma = value * saturation / 255.0;
            let segment = hue / 60.0;
            let x = chroma * (1.0 - (segment.rem_euclid(2.0) - 1.0).abs());
            let (r, g, b) = match segment as u8 {
                0 => (chroma, x, 0.0),
                1 => (x, chroma, 0.0),
                2 => (0.0, chroma, x),
                3 => (0.0, x, chroma),
                4 => (x, 0.0, chroma),
                _ => (chroma, 0.0, x),
            };
            let m = value - chroma;
            [
                (r + m).floor() as u8,
                (g + m).floor() as u8,
                (b + m).floor() as u8,
            ]
        }
        250 => [0, 0, 0],
        251 => [101, 101, 101],
        252 => [102, 102, 102],
        253 => [153, 153, 153],
        254 => [204, 204, 204],
        255 => [255, 255, 255],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pc16_default_layer_color_is_white() {
        assert_eq!(Palette::default(), Palette::Pc16);
        assert_eq!(Palette::Pc16.rgb(15), [255, 255, 255]);
        assert_eq!(Palette::Aci256.rgb(15), aci_rgb(15));
    }

    #[test]
    fn pc16_differs_from_aci_only_on_8_to_15() {
        for index in 0..=255u8 {
            let same = Palette::Pc16.rgb(index) == aci_rgb(index);
            assert_eq!(same, !(8..=15).contains(&index), "index {index}");
        }
    }

    #[test]
    fn palette_names_round_trip() {
        for palette in Palette::ALL {
            assert_eq!(Palette::from_name(palette.name()), Some(palette));
        }
        assert_eq!(Palette::from_name("PC16"), Some(Palette::Pc16));
        assert_eq!(Palette::from_name("cga"), None);
    }

    #[test]
    fn standard_primary_colors_match_autocad_definitions() {
        assert_eq!(aci_rgb(0), [255, 255, 255]);
        assert_eq!(aci_rgb(1), [255, 0, 0]);
        assert_eq!(aci_rgb(2), [255, 255, 0]);
        assert_eq!(aci_rgb(3), [0, 255, 0]);
        assert_eq!(aci_rgb(4), [0, 255, 255]);
        assert_eq!(aci_rgb(5), [0, 0, 255]);
        assert_eq!(aci_rgb(6), [255, 0, 255]);
        assert_eq!(aci_rgb(7), [255, 255, 255]);
        assert_eq!(aci_rgb(8), [128, 128, 128]);
        assert_eq!(aci_rgb(9), [192, 192, 192]);
        assert_eq!(aci_rgb(250), [0, 0, 0]);
        assert_eq!(aci_rgb(255), [255, 255, 255]);
    }
}
