//! Hatch prompt handling: named, `U` and file patterns
//! (docs/native-hatch-styles.md, docs/native-hatch-user.md).
use super::*;
use crate::geometry::{HatchRequest, HatchSpec};
use crate::hatch_pattern::{parse_pattern_file, pattern_families, user_families};
use crate::text::{angle_input, relative_point};
use std::borrow::Cow;

impl Editor {
    pub(super) fn submit_hatch(&mut self, state: InputState, line: &str) -> Result<Effect, String> {
        match state {
            InputState::HatchPattern => {
                let reply = line.trim().to_ascii_uppercase();
                if reply == "?" {
                    self.state = InputState::Command;
                    self.status = "HATCH pattern list".into();
                    return Ok(Effect::Report(hatch_pattern_report()));
                }
                let spec = HatchSpec::parse(&reply).inspect_err(|_| {
                    self.state = InputState::Command;
                })?;
                if spec.pattern == "U" {
                    self.state = InputState::HatchUserAngle(spec.style);
                    return Ok(Effect::Continue);
                }
                if !HATCH_PATTERNS.iter().any(|(name, _)| *name == spec.pattern) {
                    self.state = InputState::Command;
                    return Err(format!("unknown HATCH pattern: {}", spec.pattern));
                }
                self.state = InputState::HatchScale(HatchRequest {
                    families: Cow::Borrowed(pattern_families(&spec.pattern)?),
                    style: spec.style,
                });
                Ok(Effect::Continue)
            }
            InputState::HatchUserAngle(style) => {
                if line.contains(',') || line.trim_start().starts_with('@') {
                    let first = relative_point(line.trim(), Point { x: 0.0, y: 0.0 })?;
                    self.state = InputState::HatchUserAngleSecond(style, first);
                } else {
                    let angle = number(line.trim())?;
                    self.state = InputState::HatchUserSpacing(style, angle);
                }
                Ok(Effect::Continue)
            }
            InputState::HatchUserAngleSecond(style, first) => {
                let angle = angle_input(line.trim(), first)?;
                self.state = InputState::HatchUserSpacing(style, angle);
                Ok(Effect::Continue)
            }
            InputState::HatchUserSpacing(style, angle) => {
                if line.contains(',') || line.trim_start().starts_with('@') {
                    let first = relative_point(line.trim(), Point { x: 0.0, y: 0.0 })?;
                    self.state = InputState::HatchUserSpacingSecond(style, angle, first);
                    return Ok(Effect::Continue);
                }
                let spacing = number(line.trim())?;
                if spacing <= 0.0 {
                    return Err("HATCH line spacing must be positive".into());
                }
                self.state = InputState::HatchUserDouble(style, angle, spacing);
                Ok(Effect::Continue)
            }
            InputState::HatchUserSpacingSecond(style, angle, first) => {
                let second = relative_point(line.trim(), first)?;
                let spacing = (second.x - first.x).hypot(second.y - first.y);
                if !spacing.is_finite() || spacing <= 0.0 {
                    self.state = InputState::HatchUserSpacing(style, angle);
                    return Err("HATCH line spacing points must be distinct and finite".into());
                }
                self.state = InputState::HatchUserDouble(style, angle, spacing);
                Ok(Effect::Continue)
            }
            InputState::HatchUserDouble(style, angle, spacing) => {
                let double = match line.trim().to_ascii_uppercase().as_str() {
                    "Y" | "YES" => true,
                    "" | "N" | "NO" => false,
                    _ => return Err("answer Y or N".into()),
                };
                let request = HatchRequest {
                    families: Cow::Owned(user_families(spacing, double)),
                    style,
                };
                self.state = InputState::HatchSelection(request, 1.0, angle);
                Ok(Effect::Continue)
            }
            InputState::HatchScale(pattern) => {
                let scale = if line.trim().is_empty() {
                    1.0
                } else {
                    number(line)?
                };
                if !scale.is_finite() || scale <= 0.0 {
                    return Err("HATCH pattern scale must be positive and finite".into());
                }
                self.state = InputState::HatchAngle(pattern, scale);
                Ok(Effect::Continue)
            }
            InputState::HatchAngle(pattern, scale) => {
                let angle = if line.trim().is_empty() {
                    0.0
                } else {
                    number(line)?
                };
                if !angle.is_finite() {
                    return Err("HATCH pattern angle must be finite".into());
                }
                self.state = InputState::HatchSelection(pattern, scale, angle);
                Ok(Effect::Continue)
            }
            InputState::HatchSelection(_, _, _) => {
                let InputState::HatchSelection(pattern, scale, angle) = self.state.clone() else {
                    unreachable!()
                };
                let ids = selection(line, selectable_count(&self.drawing))?;
                self.add_hatch(&pattern, scale, angle, &ids)?;
                self.state = InputState::Command;
                self.refresh_after_edit();
                Ok(Effect::Continue)
            }
            _ => unreachable!("dispatch routes only hatch prompts"),
        }
    }

    /// At the HATCH pattern prompt, the file specification (typed case,
    /// style suffix removed) of a reply that is not `?`, `U` or a built-in
    /// name. The Session resolves it and answers through
    /// [`Editor::submit_hatch_pattern_file`]; see docs/native-hatch-user.md.
    pub fn hatch_pattern_file_request(&self, input: &str) -> Option<String> {
        if !matches!(self.state, InputState::HatchPattern) {
            return None;
        }
        let spec = HatchSpec::parse(&input.trim().to_ascii_uppercase()).ok()?;
        let builtin = HATCH_PATTERNS.iter().any(|(name, _)| *name == spec.pattern);
        if spec.pattern.is_empty() || spec.pattern == "?" || spec.pattern == "U" || builtin {
            return None;
        }
        let typed = input.split(',').next().unwrap_or_default().trim();
        Some(typed.to_owned())
    }

    /// Answer the HATCH pattern prompt with the bytes of a pattern file in
    /// ACAD.PAT syntax. The definition named by the reply's base name is
    /// used, with the reply's style. Errors return to `Command` unchanged.
    pub fn submit_hatch_pattern_file(
        &mut self,
        input: &str,
        source: &str,
        contents: &[u8],
    ) -> Result<Effect, String> {
        if !matches!(self.state, InputState::HatchPattern) {
            return Err("HATCH is not waiting for a pattern name".into());
        }
        self.state = InputState::Command;
        let spec = HatchSpec::parse(&input.trim().to_ascii_uppercase())?;
        let name = pattern_file_name(&spec.pattern);
        if name.is_empty() {
            return Err(format!("HATCH: no pattern name in {}", input.trim()));
        }
        let families = parse_pattern_file(contents, name, source)?;
        self.status = format!("Read pattern {name} from {source}");
        self.state = InputState::HatchScale(HatchRequest {
            families: Cow::Owned(families),
            style: spec.style,
        });
        Ok(Effect::Continue)
    }
}

/// The base name of a file specification: no drive, directories or extension.
fn pattern_file_name(spec: &str) -> &str {
    let file = spec.rsplit(['/', '\\', ':']).next().unwrap_or_default();
    file.rsplit_once('.').map_or(file, |(stem, _)| stem).trim()
}
