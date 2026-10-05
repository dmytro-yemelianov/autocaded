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
/// the rest looked like was up to the display driver
/// (docs/display-colours.md).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Palette {
    /// The 16-colour PC display as AutoCAD 1.4's Tecmar driver
    /// (`DSTECSS.DRV`) drives it: 1..=7 through the driver's table into IBM
    /// RGBI, every other number `n & 15` straight into RGBI, and a result
    /// equal to the black background drawn white instead. Colour 0 stays
    /// white here (the driver draws it in the background colour).
    #[default]
    Pc16,
    /// The modern 256-colour ACI table ([`aci_rgb`]) for every index.
    Aci256,
}

/// IBM RGBI, as on the 5153 monitor (index 6 is brown).
const RGBI: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00],
    [0x00, 0x00, 0xAA],
    [0x00, 0xAA, 0x00],
    [0x00, 0xAA, 0xAA],
    [0xAA, 0x00, 0x00],
    [0xAA, 0x00, 0xAA],
    [0xAA, 0x55, 0x00],
    [0xAA, 0xAA, 0xAA],
    [0x55, 0x55, 0x55],
    [0x55, 0x55, 0xFF],
    [0x55, 0xFF, 0x55],
    [0x55, 0xFF, 0xFF],
    [0xFF, 0x55, 0x55],
    [0xFF, 0x55, 0xFF],
    [0xFF, 0xFF, 0x55],
    [0xFF, 0xFF, 0xFF],
];

/// `DSTECSS.DRV` word table at DS:07A2 (file 0x0D96): colours 1..=7 (red,
/// yellow, green, cyan, blue, magenta, white) as RGBI indices.
const TECMAR_PRIMARIES: [u8; 7] = [4, 14, 2, 3, 1, 5, 15];

/// The background `DSTECSS.DRV` compares against; our canvas is black.
const BACKGROUND: u8 = 0;

impl Palette {
    pub const ALL: [Palette; 2] = [Palette::Pc16, Palette::Aci256];

    pub fn rgb(self, index: u8) -> [u8; 3] {
        match self {
            Palette::Aci256 => aci_rgb(index),
            Palette::Pc16 if index == 0 => aci_rgb(0),
            Palette::Pc16 => {
                // DSTECSS.DRV 0ADD..0B07.
                let hardware = match index {
                    1..=7 => TECMAR_PRIMARIES[usize::from(index - 1)],
                    _ => index & 0x0F,
                };
                let hardware = if hardware == BACKGROUND { 15 } else { hardware };
                RGBI[usize::from(hardware)]
            }
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
    fn pc16_follows_the_tecmar_driver() {
        // 1..=7 through the driver's table: red, yellow, green, cyan, blue,
        // magenta, white as RGBI 4, 14, 2, 3, 1, 5, 15.
        let primaries: Vec<_> = (1..=7).map(|i| Palette::Pc16.rgb(i)).collect();
        assert_eq!(primaries, [4, 14, 2, 3, 1, 5, 15].map(|i| RGBI[i]).to_vec());
        // Everything else is RGBI[n & 15]; 9 is light blue, not light red.
        assert_eq!(Palette::Pc16.rgb(8), [0x55, 0x55, 0x55]);
        assert_eq!(Palette::Pc16.rgb(9), [0x55, 0x55, 0xFF]);
        assert_eq!(Palette::Pc16.rgb(12), [0xFF, 0x55, 0x55]);
        assert_eq!(Palette::Pc16.rgb(17), RGBI[1]);
        assert_eq!(Palette::Pc16.rgb(255), RGBI[15]);
        // A result on the black background is drawn white.
        for index in [16, 32, 128, 240] {
            assert_eq!(Palette::Pc16.rgb(index), [255, 255, 255], "index {index}");
        }
        // Colour 0 keeps the white BYBLOCK stand-in.
        assert_eq!(Palette::Pc16.rgb(0), aci_rgb(0));
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
