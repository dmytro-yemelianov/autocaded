//! Orthogonal DIM primitives and session history, checked against retained DWGs.
//!
//! The finite fixture coverage and remaining placement limits are documented in
//! tests/fixtures/dim/README.md. No device-pixel rounding is applied to geometry.

use crate::{parse::format_measurement, DimStyle};
use acad_model::{Entity, Point, Units};

#[derive(Debug, Clone, Copy, PartialEq)]
enum ExtensionAxis {
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DimInput {
    pub first: Point,
    intersection: Point,
    axis: ExtensionAxis,
    sign: f64,
}

impl DimInput {
    pub fn new(first: Point, intersection: Point) -> Result<Self, String> {
        let dx = intersection.x - first.x;
        let dy = intersection.y - first.y;
        if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
            return Err("DIM extension line points must be finite and distinct".into());
        }
        // DBOBLIQ and the text-orientation cases keep an orthogonal dimension
        // despite an off-axis intersection. Dominant-axis choice also defines
        // the local policy for unmeasured directions and ties.
        let (axis, intersection, sign) = if dx.abs() >= dy.abs() {
            (
                ExtensionAxis::X,
                Point {
                    x: intersection.x,
                    y: first.y,
                },
                dx.signum(),
            )
        } else {
            (
                ExtensionAxis::Y,
                Point {
                    x: first.x,
                    y: intersection.y,
                },
                dy.signum(),
            )
        };
        Ok(Self {
            first,
            intersection,
            axis,
            sign,
        })
    }

    fn extension(self) -> Point {
        match self.axis {
            ExtensionAxis::X => Point {
                x: self.sign,
                y: 0.0,
            },
            ExtensionAxis::Y => Point {
                x: 0.0,
                y: self.sign,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DimHistory {
    input: DimInput,
    first_extension_end: Point,
    second_origin: Point,
    internal_arrows: bool,
}

impl DimHistory {
    pub fn followup(self, baseline: bool, arrow: f64) -> DimInput {
        let first = if baseline {
            self.first_extension_end
        } else {
            self.second_origin
        };
        // B always advances the line; C advances after external arrows and
        // retains the line after internal arrows in the saved orthogonal cases.
        let advance = if baseline || !self.internal_arrows {
            5.0 * arrow
        } else {
            0.0
        };
        let shifted = along(self.input.intersection, self.input.extension(), advance);
        let intersection = match self.input.axis {
            ExtensionAxis::X => Point {
                x: shifted.x,
                y: first.y,
            },
            ExtensionAxis::Y => Point {
                x: first.x,
                y: shifted.y,
            },
        };
        DimInput {
            first,
            intersection,
            ..self.input
        }
    }
}

pub(crate) struct Dimension {
    pub entities: Vec<Entity>,
    pub history: DimHistory,
}

fn along(origin: Point, direction: Point, distance: f64) -> Point {
    Point {
        x: origin.x + direction.x * distance,
        y: origin.y + direction.y * distance,
    }
}

/// Ink bounds and horizontal advances extracted from default TXT.SHP.
/// Accumulate advances between glyphs, but use the last stroke, not its advance,
/// as the text edge. Unknown glyphs use a numeric-width fallback; those strings
/// have no native placement claim. Space has advance and no ink.
fn text_width(text: &str, height: f64) -> f64 {
    let mut pen = 0.0_f64;
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    for glyph in text.chars() {
        let (advance, ink) = u32::from(glyph)
            .checked_sub(32)
            .and_then(|index| crate::txt_metrics::TXT_METRICS.get(index as usize))
            .copied()
            .flatten()
            // Preserve the existing fallback for characters absent from TXT.
            // The renderer reports missing glyphs; no native placement is claimed.
            .unwrap_or((20.0, Some((0.0, 14.0))));
        if let Some((lo, hi)) = ink {
            left = left.min(pen + lo);
            right = right.max(pen + hi);
        }
        pen += advance;
    }
    if left.is_finite() {
        (right - left) * height / crate::txt_metrics::TXT_CAP_HEIGHT
    } else {
        0.0
    }
}

pub(crate) fn dimension_geometry(
    input: DimInput,
    second: Point,
    text: Option<&str>,
    style: DimStyle,
    units: Units,
) -> Result<Dimension, String> {
    let extension = input.extension();
    let (end, offset, side) = match input.axis {
        ExtensionAxis::X => (
            Point {
                x: input.intersection.x,
                y: second.y,
            },
            second.y - input.first.y,
            Point { x: 1.0, y: 0.0 },
        ),
        ExtensionAxis::Y => (
            Point {
                x: second.x,
                y: input.intersection.y,
            },
            second.x - input.first.x,
            Point { x: 0.0, y: -1.0 },
        ),
    };
    let arrow = style.arrow_size;
    let height = 1.5 * arrow;
    if !offset.is_finite()
        || offset == 0.0
        || !second.x.is_finite()
        || !second.y.is_finite()
        || !height.is_finite()
        || height <= 0.0
    {
        return Err("DIM needs distinct finite extension origins and a finite arrow size".into());
    }
    let length = offset.abs();
    let direction = match input.axis {
        ExtensionAxis::X => Point {
            x: 0.0,
            y: offset.signum(),
        },
        ExtensionAxis::Y => Point {
            x: offset.signum(),
            y: 0.0,
        },
    };
    let middle = Point {
        x: (input.intersection.x + end.x) / 2.0,
        y: (input.intersection.y + end.y) / 2.0,
    };
    let value = text
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format_measurement(length, units));
    let width = text_width(&value, height);
    // PFTLOW/AT/HI distinguish < from <= at the exact width+6A boundary.
    // Applying this comparator beyond the finite measured cases is local policy.
    let internal = length >= width + 6.0 * arrow;
    let horizontal = if internal {
        style.inside_horizontal_text
    } else {
        style.outside_horizontal_text
    };
    let rotate = !horizontal && matches!(input.axis, ExtensionAxis::X);
    let gap_width = if matches!(input.axis, ExtensionAxis::X) && !rotate {
        height
    } else {
        width
    };
    let first_extension_end = along(input.intersection, extension, arrow);
    let mut entities = vec![
        Entity::Line {
            start: input.first,
            end: first_extension_end,
        },
        Entity::Line {
            start: second,
            end: along(end, extension, arrow),
        },
    ];
    let (base_start, base_end) = if internal {
        let a = along(input.intersection, direction, arrow);
        let b = along(end, direction, -arrow);
        entities.push(Entity::Line {
            start: a,
            end: along(middle, direction, -gap_width / 2.0 - arrow),
        });
        entities.push(Entity::Line {
            start: b,
            end: along(middle, direction, gap_width / 2.0 + arrow),
        });
        (a, b)
    } else {
        let a = along(input.intersection, direction, -arrow);
        let b = along(end, direction, arrow);
        entities.push(Entity::Line {
            start: b,
            end: along(end, direction, 2.0 * arrow),
        });
        entities.push(Entity::Line {
            start: a,
            end: along(input.intersection, direction, -2.0 * arrow),
        });
        (a, b)
    };
    let arrows = if internal {
        [(input.intersection, base_start), (end, base_end)]
    } else {
        [(end, base_end), (input.intersection, base_start)]
    };
    for (tip, base) in arrows {
        entities.push(Entity::Solid {
            p1: along(base, side, arrow / 6.0),
            p2: along(base, side, -arrow / 6.0),
            p3: tip,
            p4: tip,
        });
    }
    // Horizontal text across a vertical dimension line (X extensions) is
    // centred on the line unless its half-width exceeds the shorter extension
    // line's reach, min(|first.x - line.x|, |second.x - line.x|) - A. The
    // excess pushes the text centre along the extension direction. Measured
    // by the in-tree original (crates/acad-oracle/tests/dim_arrows.rs,
    // docs/native-dim.md); the retained B 3A offset is this rule's case
    // reach = 4A - A.
    let crossing_text_x = || {
        let reach = (input.first.x - input.intersection.x)
            .abs()
            .min((second.x - end.x).abs())
            - arrow;
        let push = (width / 2.0 - reach).max(0.0);
        middle.x + input.sign * push - width / 2.0
    };
    let origin = if internal {
        if rotate {
            Point {
                x: middle.x + height / 2.0,
                y: middle.y - width / 2.0,
            }
        } else {
            Point {
                x: match input.axis {
                    ExtensionAxis::X => crossing_text_x(),
                    ExtensionAxis::Y => middle.x - width / 2.0,
                },
                y: middle.y - height / 2.0,
            }
        }
    } else {
        match input.axis {
            ExtensionAxis::X => Point {
                x: if rotate {
                    middle.x + height / 2.0
                } else {
                    crossing_text_x()
                },
                y: end.y + offset.signum() * 3.0 * arrow
                    - if offset < 0.0 {
                        if rotate {
                            width
                        } else {
                            height
                        }
                    } else {
                        0.0
                    },
            },
            ExtensionAxis::Y => Point {
                x: end.x + offset.signum() * 3.0 * arrow - if offset < 0.0 { width } else { 0.0 },
                y: middle.y - height / 2.0,
            },
        }
    };
    entities.push(Entity::Text {
        origin,
        height,
        rotation_deg: if rotate { 90.0 } else { 0.0 },
        value,
    });
    // Reject overflow before the caller commits any drawing or history changes.
    if entities.iter().any(|entity| match entity {
        Entity::Line { start, end } => !finite(*start) || !finite(*end),
        Entity::Solid { p1, p2, p3, p4 } => [p1, p2, p3, p4].iter().any(|p| !finite(**p)),
        Entity::Text { origin, .. } => !finite(*origin),
        _ => false,
    }) {
        return Err("DIM geometry exceeds the finite coordinate range".into());
    }
    Ok(Dimension {
        entities,
        history: DimHistory {
            input,
            first_extension_end,
            second_origin: second,
            internal_arrows: internal,
        },
    })
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

#[cfg(test)]
mod text_metrics_tests {
    use super::text_width;

    #[test]
    fn alphabetic_widths_use_ink_and_intermediate_advances() {
        assert_eq!(text_width("MMMM", 21.0), 88.0);
        assert_eq!(text_width("mmmm", 21.0), 112.0);
        assert_eq!(text_width("?", 21.0), 11.0);
        assert_eq!(text_width("+", 21.0), 18.0);
    }

    #[test]
    fn spacing_contributes_between_ink_but_not_at_the_text_edges() {
        assert_eq!(text_width(" M  ", 21.0), 16.0);
        assert_eq!(text_width("M m", 21.0), 65.0);
        assert_eq!(text_width("   ", 21.0), 0.0);
    }

    #[test]
    fn numeric_and_retained_label_metrics_remain_exact() {
        for (label, width) in [
            ("1.0000", 100.0),
            ("3.0000", 104.0),
            ("1111", 53.0),
            ("8888", 74.0),
            ("1.01", 51.0),
            ("WWWW", 92.0),
            ("iiii", 26.0),
        ] {
            assert_eq!(text_width(label, 21.0), width, "{label}");
        }
    }

    #[test]
    fn missing_font_characters_retain_the_explicit_local_fallback() {
        assert_eq!(crate::txt_metrics::TXT_METRICS.iter().flatten().count(), 94);
        assert_eq!(text_width("`", 21.0), 14.0);
        assert_eq!(text_width("Я", 21.0), 14.0);
    }
}
