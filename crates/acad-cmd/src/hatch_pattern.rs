//! Line families transcribed from the retained System/ACAD.PAT, `U` patterns
//! and the bounded ACAD.PAT-syntax parser for external pattern files
//! (docs/native-hatch-user.md). Positive entries draw strokes, negative
//! entries gaps, and zero entries dots.

use crate::MAX_ARRAY_ENTITIES;
use acad_model::{EngineLimits, Point};
use std::borrow::Cow;

/// Total HATCH work: every sweep row and every dash cycle visited, across
/// all families. Bounds files whose rows draw nothing (or nothing visible),
/// which the emitted-stroke budget alone cannot catch.
pub(crate) const MAX_HATCH_WORK: usize = EngineLimits::DEFAULT_1983.max_hatch_work;

#[derive(Default)]
pub(crate) struct HatchWork {
    used: usize,
    /// Boundary items examined by sweep rows. Not charged against the
    /// budget; it measures that rows skip items they cannot cross.
    visits: usize,
}

impl HatchWork {
    /// Charge `units` before doing them; fails once the total would exceed
    /// [`MAX_HATCH_WORK`].
    pub fn charge(&mut self, units: usize) -> Result<(), String> {
        match self.used.checked_add(units) {
            Some(total) if total <= MAX_HATCH_WORK => {
                self.used = total;
                Ok(())
            }
            _ => Err(format!(
                "HATCH work exceeds the limit of {MAX_HATCH_WORK} rows and dash cycles"
            )),
        }
    }

    /// Record boundary items examined by one sweep row.
    pub fn visit(&mut self, items: usize) {
        self.visits = self.visits.saturating_add(items);
    }

    #[cfg(test)]
    pub fn used(&self) -> usize {
        self.used
    }

    #[cfg(test)]
    pub fn visits(&self) -> usize {
        self.visits
    }
}

pub(crate) struct DashPattern {
    period: f64,
    strokes: Vec<(f64, f64)>,
}

impl DashPattern {
    pub fn new(entries: &[f64], scale: f64) -> Result<Option<Self>, String> {
        if entries.is_empty() {
            return Ok(None);
        }
        let mut period = 0.0;
        let mut strokes = Vec::new();
        for entry in entries {
            if *entry == 0.0 {
                strokes.push((period, period));
                continue;
            }
            let length = entry.abs() * scale;
            let next = period + length;
            if !next.is_finite() || length <= 0.0 || next <= period {
                return Err("HATCH dash lengths are outside the finite supported range".into());
            }
            if *entry > 0.0 {
                strokes.push((period, next));
            }
            period = next;
        }
        if period == 0.0 {
            return Err("HATCH dash repeat must have a positive finite period".into());
        }
        Ok(Some(Self { period, strokes }))
    }

    /// Clip the globally phased repeat to a boundary interval, including negative
    /// coordinates. Each interval uses the same phase, so holes never restart it.
    pub fn for_each_stroke(
        &self,
        start: f64,
        end: f64,
        phase: f64,
        work: &mut HatchWork,
        mut emit: impl FnMut(f64, f64) -> Result<(), String>,
    ) -> Result<(), String> {
        if !phase.is_finite() {
            return Err("HATCH row drift exceeds the finite coordinate range".into());
        }
        let phase = phase.rem_euclid(self.period);
        let lower = if self.strokes.iter().any(|(a, b)| a == b) {
            start - 1e-10
        } else {
            start
        };
        let first = ((lower - phase) / self.period).floor();
        let last = ((end - phase) / self.period).floor();
        // Exact adjacent cycle indices are needed for non-overlapping repeats.
        const EXACT_INDEX: f64 = (1_u64 << 53) as f64;
        if !first.is_finite() || !last.is_finite() || first <= -EXACT_INDEX || last >= EXACT_INDEX {
            return Err("HATCH dash cycle indices exceed the supported range".into());
        }
        if last - first > MAX_ARRAY_ENTITIES as f64 {
            return Err("HATCH exceeds the supported dash cycle count".into());
        }
        work.charge((last - first) as usize + 1)?;
        for cycle in first as i64..=last as i64 {
            let base = phase + cycle as f64 * self.period;
            for (a, b) in &self.strokes {
                if a == b {
                    let dot = base + a;
                    // Half-open intervals avoid emitting the same dot twice
                    // when adjacent clipped intervals share an endpoint.
                    // Reuse the sweep's boundary tolerance: projecting a
                    // rotated boundary can place its lower endpoint a few
                    // ULPs beyond a dot that lies exactly on that edge.
                    if start - 1e-10 <= dot && dot < end - 1e-10 {
                        emit(dot, dot)?;
                    }
                    continue;
                }
                let a = (base + a).max(start);
                let b = (base + b).min(end);
                if a < b {
                    emit(a, b)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HatchFamily {
    pub angle: f64,
    pub origin: Point,
    pub spacing: f64,
    pub drift: f64,
    pub dashes: Cow<'static, [f64]>,
}

const fn family(angle: f64, y: f64, spacing: f64) -> HatchFamily {
    HatchFamily {
        angle,
        origin: Point { x: 0.0, y },
        spacing,
        drift: 0.0,
        dashes: Cow::Borrowed(&[]),
    }
}

/// HLP `U`: continuous lines through the origin at the user angle, plus the
/// perpendicular family when double-hatching (the original's order).
pub(crate) fn user_families(spacing: f64, double: bool) -> Vec<HatchFamily> {
    let angles: &[f64] = if double { &[0.0, 90.0] } else { &[0.0] };
    angles
        .iter()
        .map(|angle| family(*angle, 0.0, spacing))
        .collect()
}

/// Largest pattern file the editor parses (and the Session reads).
pub const MAX_HATCH_PATTERN_FILE_BYTES: usize = 262_144;
const MAX_PATTERN_FILE_LINES: usize = 8_192;
const MAX_PATTERN_ROWS: usize = 64;
const MAX_PATTERN_DASHES: usize = 16;

/// Find `*name` in ACAD.PAT-syntax `contents` and parse its rows. Text ends
/// at the DOS end-of-file byte; only the requested definition is validated.
/// `source` names the file in errors, which give 1-based line numbers.
pub(crate) fn parse_pattern_file(
    contents: &[u8],
    name: &str,
    source: &str,
) -> Result<Vec<HatchFamily>, String> {
    if contents.len() > MAX_HATCH_PATTERN_FILE_BYTES {
        return Err(format!(
            "HATCH: {source} exceeds the {MAX_HATCH_PATTERN_FILE_BYTES}-byte pattern file limit"
        ));
    }
    let text = contents
        .split(|byte| *byte == 0x1A)
        .next()
        .unwrap_or_default();
    let mut families: Option<Vec<HatchFamily>> = None;
    for (index, line) in text.split(|byte| *byte == b'\n').enumerate() {
        let number = index + 1;
        let at = |message: String| format!("HATCH: {source} line {number}: {message}");
        if number > MAX_PATTERN_FILE_LINES {
            return Err(at(format!("more than {MAX_PATTERN_FILE_LINES} lines")));
        }
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(header) = line.strip_prefix(b"*") {
            if families.is_some() {
                break;
            }
            let header = header
                .split(|byte| *byte == b',')
                .next()
                .unwrap_or_default();
            if header.trim_ascii().eq_ignore_ascii_case(name.as_bytes()) {
                families = Some(Vec::new());
            }
            continue;
        }
        let Some(rows) = families.as_mut() else {
            continue;
        };
        let line = line.trim_ascii();
        if line.is_empty() || line.starts_with(b";") {
            continue;
        }
        let Ok(line) = std::str::from_utf8(line).map_err(drop).and_then(|text| {
            if text.is_ascii() {
                Ok(text)
            } else {
                Err(())
            }
        }) else {
            return Err(at("not ASCII text".into()));
        };
        if rows.len() == MAX_PATTERN_ROWS {
            return Err(at(format!("more than {MAX_PATTERN_ROWS} rows in *{name}")));
        }
        rows.push(pattern_row(line).map_err(at)?);
    }
    match families {
        None => Err(format!("HATCH: {source} has no *{name} definition")),
        Some(rows) if rows.is_empty() => Err(format!("HATCH: {source}: *{name} has no rows")),
        Some(rows) => Ok(rows),
    }
}

/// `angle, x-origin, y-origin, delta-x, delta-y [, dash ...]`.
fn pattern_row(line: &str) -> Result<HatchFamily, String> {
    let values = line
        .split(',')
        .map(|field| {
            let field = field.trim();
            field
                .parse::<f64>()
                .map_err(|_| format!("invalid number: {field}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let [angle, x, y, drift, spacing, dashes @ ..] = values.as_slice() else {
        return Err("expected angle, x-origin, y-origin, delta-x, delta-y".into());
    };
    if !values.iter().all(|value| value.is_finite()) {
        return Err("values must be finite".into());
    }
    if *spacing <= 0.0 {
        return Err("delta-y must be positive".into());
    }
    if dashes.len() > MAX_PATTERN_DASHES {
        return Err(format!("more than {MAX_PATTERN_DASHES} dash entries"));
    }
    if !dashes.is_empty() && dashes.iter().all(|dash| *dash == 0.0) {
        return Err("dash entries must not all be zero".into());
    }
    // No dash list is a continuous row; a list of gaps only draws nothing.
    if !dashes.is_empty() && dashes.iter().all(|dash| *dash < 0.0) {
        return Err("dash entries need a stroke (positive) or dot (zero)".into());
    }
    Ok(HatchFamily {
        angle: *angle,
        origin: Point { x: *x, y: *y },
        spacing: *spacing,
        drift: *drift,
        dashes: Cow::Owned(dashes.to_vec()),
    })
}

/// The default built-in ACAD.PAT pattern library transcribed from the original
/// 1983 AutoCAD 1.4 System disk, embedded as a fallback when no external file is provided.
pub const DEFAULT_ACAD_PAT: &[u8] = include_bytes!("../../../assets/acad.pat");

pub(crate) fn pattern_families(pattern: &str) -> Result<Vec<HatchFamily>, String> {
    parse_pattern_file(DEFAULT_ACAD_PAT, pattern, "ACAD.PAT")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intervals(entries: &[f64], start: f64, end: f64) -> Vec<(f64, f64)> {
        let pattern = DashPattern::new(entries, 1.0).unwrap().unwrap();
        let mut result = Vec::new();
        pattern
            .for_each_stroke(start, end, 0.0, &mut HatchWork::default(), |a, b| {
                result.push((a, b));
                Ok(())
            })
            .unwrap();
        result
    }

    #[test]
    fn work_is_charged_per_dash_cycle_before_the_cycles_run() {
        let pattern = DashPattern::new(&[0.25, -0.25], 1.0).unwrap().unwrap();
        let mut work = HatchWork::default();
        pattern
            .for_each_stroke(0.0, 2.0, 0.0, &mut work, |_, _| Ok(()))
            .unwrap();
        assert_eq!(work.used(), 5);
        let mut full = HatchWork::default();
        full.charge(MAX_HATCH_WORK - 4).unwrap();
        let mut emitted = 0;
        assert!(pattern
            .for_each_stroke(0.0, 2.0, 0.0, &mut full, |_, _| {
                emitted += 1;
                Ok(())
            })
            .unwrap_err()
            .contains("work exceeds"));
        assert_eq!(emitted, 0);
        assert!(full.charge(4).is_ok() && full.charge(1).is_err());
    }

    #[test]
    fn leading_gaps_and_negative_cycles_keep_their_drawn_interval() {
        assert_eq!(
            intervals(&[-0.25, 0.125], -0.375, 0.375),
            vec![(-0.125, 0.0), (0.25, 0.375)]
        );
    }

    #[test]
    fn multiple_dash_lengths_clip_the_first_stroke_without_restarting_the_cycle() {
        assert_eq!(
            intervals(&[0.125, -0.125, 0.25, -0.5], 0.3, 1.6),
            vec![(0.3, 0.5), (1.0, 1.125), (1.25, 1.5)]
        );
    }

    #[test]
    fn dash_lengths_reject_zero_period_nonfinite_underflow_and_unrepresentable_sums() {
        for entries in [
            &[0.0][..],
            &[f64::NAN],
            &[f64::INFINITY],
            &[1e308, 1e308],
            &[1e300, 0.125],
        ] {
            assert!(DashPattern::new(entries, 1.0).is_err(), "{entries:?}");
        }
        assert!(DashPattern::new(&[0.125, -0.125], f64::from_bits(1)).is_err());
    }

    #[test]
    fn dots_keep_zero_length_and_repeat_at_negative_coordinates() {
        assert_eq!(
            intervals(&[0.0, -0.25], -0.5, 0.5),
            vec![(-0.5, -0.5), (-0.25, -0.25), (0.0, 0.0), (0.25, 0.25)]
        );
        assert_eq!(
            intervals(&[0.25, -0.25, 0.0, -0.25, 0.0, -0.25], 0.1, 1.0),
            vec![(0.1, 0.25), (0.5, 0.5), (0.75, 0.75)]
        );
    }

    #[test]
    fn projected_dot_endpoints_use_the_same_tolerance_for_cycles_and_clipping() {
        assert_eq!(
            intervals(&[0.0, -0.25], 0.25 + f64::EPSILON, 0.5 + f64::EPSILON),
            vec![(0.25, 0.25)]
        );
    }
}
