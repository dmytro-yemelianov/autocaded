use acad_model::{Point, UnitFormat, Units};

pub(crate) fn positive_count(input: &str, name: &str) -> Result<usize, String> {
    let count = input
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if count == 0 {
        return Err(format!("{name} must be a positive integer"));
    }
    Ok(count)
}

pub(crate) fn positive_word(input: &str, name: &str) -> Result<u16, String> {
    let count = positive_count(input, name)?;
    u16::try_from(count).map_err(|_| format!("{name} exceeds 65535"))
}

pub(crate) fn number(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .or_else(|| parse_feet_inches(text))
        .filter(|x| x.is_finite())
        .ok_or_else(|| format!("invalid number: {text}"))
}

/// Parse AutoCAD's Engineering/Architectural distance syntax as inches. This
/// is deliberately accepted alongside decimal syntax; the active `UNITS`
/// selection controls display, while legacy drawings and scripts commonly
/// contain a mixture of numeric spellings.
pub(crate) fn parse_feet_inches(text: &str) -> Option<f64> {
    let (feet, inches) = text.trim().strip_suffix('"')?.split_once('\'')?;
    let feet = feet.trim().parse::<f64>().ok()?;
    let inches = inches.trim().trim_start_matches('-').trim();
    let inches = if let Some((whole, fraction)) = inches.split_once(char::is_whitespace) {
        whole.trim().parse::<f64>().ok()? + parse_fraction(fraction.trim())?
    } else if inches.contains('/') {
        parse_fraction(inches)?
    } else {
        inches.parse::<f64>().ok()?
    };
    let sign = if feet.is_sign_negative() { -1.0 } else { 1.0 };
    Some(sign * (feet.abs() * 12.0 + inches))
}

pub(crate) fn parse_fraction(text: &str) -> Option<f64> {
    let (numerator, denominator) = text.split_once('/')?;
    let numerator = numerator.trim().parse::<f64>().ok()?;
    let denominator = denominator.trim().parse::<f64>().ok()?;
    (denominator != 0.0).then_some(numerator / denominator)
}

pub(crate) fn format_measurement(value: f64, units: Units) -> String {
    match units.format {
        UnitFormat::Scientific => format!("{:.*E}", usize::from(units.precision), value),
        UnitFormat::Decimal => format!("{:.*}", usize::from(units.precision), value),
        UnitFormat::Engineering => format_feet_inches(value, units.precision, None),
        UnitFormat::Architectural => format_feet_inches(value, 0, Some(units.precision)),
    }
}

pub(crate) fn format_feet_inches(value: f64, decimals: u16, denominator: Option<u16>) -> String {
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let magnitude = value.abs();
    let feet = (magnitude / 12.0).floor() as u64;
    let inches = magnitude - feet as f64 * 12.0;
    match denominator {
        None => format!("{sign}{feet}'-{inches:.*}\"", usize::from(decimals)),
        Some(denominator) => {
            let scaled = (inches * f64::from(denominator)).round() as u64;
            let whole = scaled / u64::from(denominator);
            let numerator = scaled % u64::from(denominator);
            if numerator == 0 {
                format!("{sign}{feet}'-{whole}\"")
            } else if whole == 0 {
                let divisor = gcd(numerator, u64::from(denominator));
                format!(
                    "{sign}{feet}'-{}/{}\"",
                    numerator / divisor,
                    u64::from(denominator) / divisor
                )
            } else {
                let divisor = gcd(numerator, u64::from(denominator));
                format!(
                    "{sign}{feet}'-{whole} {}/{}\"",
                    numerator / divisor,
                    u64::from(denominator) / divisor
                )
            }
        }
    }
}

pub(crate) fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

pub(crate) fn point(text: &str) -> Result<Point, String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("expected x,y point: {text}"))?;
    Ok(Point {
        x: number(x.trim())?,
        y: number(y.trim())?,
    })
}

pub(crate) fn point_from(text: &str, base: Point) -> Result<Point, String> {
    let Some(relative) = text.strip_prefix('@') else {
        return point(text);
    };
    if let Some((distance, angle)) = relative.split_once('<') {
        let distance = number(distance.trim())?;
        let angle = number(angle.trim())?;
        let (sin, cos) = sin_cos_degrees(angle);
        return Ok(Point {
            x: base.x + distance * cos,
            y: base.y + distance * sin,
        });
    }
    let delta = point(relative)?;
    Ok(Point {
        x: base.x + delta.x,
        y: base.y + delta.y,
    })
}

pub(crate) fn sin_cos_degrees(degrees: f64) -> (f64, f64) {
    match degrees.rem_euclid(360.0) {
        0.0 => (0.0, 1.0),
        90.0 => (1.0, 0.0),
        180.0 => (0.0, -1.0),
        270.0 => (-1.0, 0.0),
        _ => degrees.to_radians().sin_cos(),
    }
}

pub(crate) fn layer_index(text: &str) -> Result<u8, String> {
    let value = text
        .parse::<u16>()
        .map_err(|_| format!("invalid layer index: {text}"))?;
    u8::try_from(value)
        .ok()
        .filter(|layer| *layer < 128)
        .ok_or_else(|| format!("layer index out of range: {value}"))
}

pub(crate) fn color_index(text: &str) -> Result<u8, String> {
    let value = text
        .parse::<u16>()
        .map_err(|_| format!("invalid color index: {text}"))?;
    u8::try_from(value)
        .ok()
        .filter(|color| *color < 255)
        .ok_or_else(|| format!("color index out of range: {value}"))
}

pub(crate) fn parse_toggle(text: &str) -> Result<bool, String> {
    match text.to_ascii_uppercase().as_str() {
        "ON" | "YES" | "1" => Ok(true),
        "OFF" | "NO" | "0" => Ok(false),
        _ => Err(format!("expected ON or OFF: {text}")),
    }
}

pub(crate) fn parse_mode(
    text: &str,
    previous: acad_model::Mode,
) -> Result<acad_model::Mode, String> {
    match text.to_ascii_uppercase().as_str() {
        "ON" | "YES" => Ok(acad_model::Mode {
            on: true,
            spacing: previous.spacing,
        }),
        "OFF" | "NO" => Ok(acad_model::Mode {
            on: false,
            spacing: previous.spacing,
        }),
        _ => {
            let spacing = number(text)?;
            if spacing <= 0.0 {
                return Err("spacing must be positive".into());
            }
            Ok(acad_model::Mode { on: true, spacing })
        }
    }
}
