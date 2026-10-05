//! Nonblocking command-script executor (docs/native-scripts.md).
//!
//! The Session owns one script at a time and a cursor of the next unread
//! byte. Work happens only in [`Session::pump_script`], which a host calls
//! from its event loop (GUI `about_to_wait`) or API request; it executes at
//! most [`SCRIPT_ITEMS_PER_PUMP`] items and never sleeps. DELAY records a
//! deadline on the injectable [`ScriptClock`]; the host asks
//! [`Session::script_wake_in`] when to pump again.
use super::external_insert::{read_bounded_kind, resolve_file, FileKind};
use super::*;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
// `std::time::Instant::now` panics on wasm32-unknown-unknown; on native
// targets `web_time` re-exports `std::time`.
use web_time::Instant;

/// Largest script file read. Larger files are refused before any item runs.
pub const MAX_SCRIPT_BYTES: u64 = 256 * 1024;
/// Most items one pump executes, so a long script cannot starve input.
pub const SCRIPT_ITEMS_PER_PUMP: usize = 64;

const SCRIPT_FILE: FileKind = FileKind {
    command: "SCRIPT",
    noun: "script file",
    extensions: &["SCR", "scr"],
};

/// Monotonic time source for DELAY deadlines. Tests inject a fake clock.
pub trait ScriptClock: Send + Sync {
    /// Time elapsed since an arbitrary fixed origin; must never decrease.
    fn now(&self) -> Duration;
}

/// Host monotonic clock.
pub struct SystemClock(Instant);
impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl ScriptClock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

/// Why a script stopped before its end; RESUME continues after any of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptInterrupt {
    /// An item was rejected; the cursor is already past that item.
    Error,
    /// Keyboard, mouse or API input arrived while the script was active.
    Input,
    /// Cancel (Esc / Ctrl-C / API cancel) arrived while the script was active.
    Cancel,
}
impl ScriptInterrupt {
    fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Input => "input",
            Self::Cancel => "cancel",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptPhase {
    /// Items are due now.
    Running,
    /// Paused by DELAY until this clock time.
    Delaying { until: Duration },
    /// Waiting for RESUME.
    Interrupted {
        cause: ScriptInterrupt,
        /// Source line of the failing item (errors) or of the next item.
        line: usize,
        message: String,
    },
}

pub(crate) struct ScriptRun {
    path: PathBuf,
    text: String,
    /// Byte offset of the next unread item: the exact resume point.
    cursor: usize,
    /// 1-based source line of `cursor`.
    line: usize,
    /// Items submitted so far.
    items: usize,
    phase: ScriptPhase,
}

impl ScriptRun {
    fn new(path: PathBuf, text: String) -> Self {
        Self {
            path,
            text,
            cursor: 0,
            line: 1,
            items: 0,
            phase: ScriptPhase::Running,
        }
    }

    /// Next Return-terminated item. CR, LF and CR LF each end one item;
    /// outside literal prompts each space or tab also ends one. An
    /// unterminated final piece is returned as `Err(tail)` and consumed.
    fn next_item(&mut self, literal: bool) -> Option<Result<(String, usize), String>> {
        let rest = &self.text[self.cursor..];
        if rest.is_empty() {
            return None;
        }
        let line = self.line;
        let end =
            rest.find(|c: char| matches!(c, '\r' | '\n') || (!literal && matches!(c, ' ' | '\t')));
        let Some(end) = end else {
            self.cursor = self.text.len();
            return Some(Err(rest.to_owned()));
        };
        let item = rest[..end].to_owned();
        let terminator = &rest[end..];
        let consumed = if terminator.starts_with("\r\n") { 2 } else { 1 };
        if terminator.starts_with(['\r', '\n']) {
            self.line += 1;
        }
        self.cursor += end + consumed;
        Some(Ok((item, line)))
    }
}

/// Outcome of one [`Session::pump_script`] call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScriptPump {
    /// The script exited the program (END, QUIT Y, ...).
    pub quit: bool,
    /// At least one item ran or the phase changed; hosts should redraw.
    pub progressed: bool,
}

impl Session {
    /// Replace the clock used for DELAY deadlines (tests, deterministic hosts).
    pub fn set_script_clock(&mut self, clock: Arc<dyn ScriptClock>) {
        self.script_clock = clock;
    }

    pub(crate) fn script_clock(&self) -> Arc<dyn ScriptClock> {
        self.script_clock.clone()
    }

    /// Resolve and read a script, then make it the only script, due now.
    /// Failures leave any existing (interrupted) script untouched.
    pub fn start_script(&mut self, spec: &str) -> Result<(), String> {
        self.refuse_at_main_menu()?;
        if self.script_stepping {
            return Err("SCRIPT: a running command script cannot start another".into());
        }
        let spec = spec.trim();
        if spec.is_empty() {
            return Err("SCRIPT requires a file name".into());
        }
        let directory = self.document.path.as_deref().and_then(Path::parent);
        let cwd = std::env::current_dir().map_err(|e| format!("SCRIPT: current directory: {e}"))?;
        let path = resolve_file(
            spec,
            directory,
            &cwd,
            &crate::files::drive_roots(),
            SCRIPT_FILE,
        )?;
        let bytes = read_bounded_kind(&path, MAX_SCRIPT_BYTES, SCRIPT_FILE)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| format!("SCRIPT: {} is not UTF-8 text", path.display()))?;
        self.status = format!("Running script {}", path.display());
        self.script = Some(ScriptRun::new(path, text));
        Ok(())
    }

    /// Discard any script (running, delaying or interrupted).
    pub fn stop_script(&mut self) -> bool {
        self.script.take().is_some()
    }

    /// User input while a script is active stops it before that input is
    /// processed; the input itself is then handled normally.
    pub(crate) fn interrupt_script(&mut self, cause: ScriptInterrupt) {
        if self.script_stepping {
            return;
        }
        let Some(run) = &mut self.script else {
            return;
        };
        if matches!(run.phase, ScriptPhase::Interrupted { .. }) {
            return;
        }
        let message = format!(
            "Script interrupted by {} before line {}; RESUME continues",
            cause.name(),
            run.line
        );
        run.phase = ScriptPhase::Interrupted {
            cause,
            line: run.line,
            message: message.clone(),
        };
        self.status = message;
    }

    /// Execute due script items: at most [`SCRIPT_ITEMS_PER_PUMP`], stopping
    /// early at DELAY, an interruption, the end of the script or an exit.
    pub fn pump_script(&mut self) -> ScriptPump {
        let mut pump = ScriptPump::default();
        let now = self.script_clock.now();
        match self.script.as_mut().map(|run| &mut run.phase) {
            None | Some(ScriptPhase::Interrupted { .. }) => return pump,
            Some(ScriptPhase::Delaying { until }) if now < *until => return pump,
            Some(phase @ ScriptPhase::Delaying { .. }) => {
                *phase = ScriptPhase::Running;
                pump.progressed = true;
            }
            Some(ScriptPhase::Running) => {}
        }
        for _ in 0..SCRIPT_ITEMS_PER_PUMP {
            let literal = self.editor.accepts_literal_text();
            let run = self.script.as_mut().expect("running script");
            let Some(next) = run.next_item(literal) else {
                let run = self.script.take().expect("running script");
                self.status = format!("Script {} complete", run.path.display());
                return pump;
            };
            pump.progressed = true;
            let (item, line) = match next {
                Ok(item) => item,
                Err(tail) => {
                    // The original leaves unterminated keys as typed input.
                    let run = self.script.take().expect("running script");
                    self.input = tail;
                    self.status = format!("Script {} complete", run.path.display());
                    return pump;
                }
            };
            run.items += 1;
            self.script_stepping = true;
            let result = self.submit_line(&item);
            self.script_stepping = false;
            match result {
                Ok(true) => {
                    self.script = None;
                    pump.quit = true;
                    return pump;
                }
                Ok(false) => {}
                Err(error) => {
                    let Some(run) = &mut self.script else {
                        return pump;
                    };
                    let message = format!(
                        "Script {} line {line}: {error}; RESUME continues at line {}",
                        run.path.display(),
                        run.line
                    );
                    run.phase = ScriptPhase::Interrupted {
                        cause: ScriptInterrupt::Error,
                        line,
                        message: message.clone(),
                    };
                    self.status = message;
                    return pump;
                }
            }
            if !matches!(
                self.script.as_ref().map(|run| &run.phase),
                Some(ScriptPhase::Running)
            ) {
                return pump;
            }
        }
        pump
    }

    /// How long a host may wait before the next useful [`Self::pump_script`]:
    /// zero while items are due, the DELAY remainder while paused, `None`
    /// when no script work is pending (idle or interrupted).
    pub fn script_wake_in(&self) -> Option<Duration> {
        match &self.script.as_ref()?.phase {
            ScriptPhase::Running => Some(Duration::ZERO),
            ScriptPhase::Delaying { until } => Some(until.saturating_sub(self.script_clock.now())),
            ScriptPhase::Interrupted { .. } => None,
        }
    }

    pub fn script_phase(&self) -> Option<&ScriptPhase> {
        self.script.as_ref().map(|run| &run.phase)
    }

    /// Shared API/MCP description of the script executor.
    pub fn script_status(&self) -> serde_json::Value {
        let Some(run) = &self.script else {
            return serde_json::json!({"state":"idle"});
        };
        let (state, delay, interrupt) = match &run.phase {
            ScriptPhase::Running => ("running", None, None),
            ScriptPhase::Delaying { until } => (
                "delaying",
                Some(until.saturating_sub(self.script_clock.now()).as_millis() as u64),
                None,
            ),
            ScriptPhase::Interrupted {
                cause,
                line,
                message,
            } => (
                "interrupted",
                None,
                Some(serde_json::json!({"cause":cause.name(),"line":line,"message":message})),
            ),
        };
        serde_json::json!({"state":state,"path":run.path.to_string_lossy(),"next_line":run.line,
            "next_offset":run.cursor,"bytes":run.text.len(),"items_submitted":run.items,
            "delay_remaining_ms":delay,"interrupt":interrupt})
    }

    /// Editor script effects. DELAY and RESUME are inert outside a script;
    /// SCRIPT from inside a script is refused, which interrupts it.
    pub(crate) fn perform_script_effect(&mut self, effect: acad_cmd::Effect) -> Result<(), String> {
        match effect {
            acad_cmd::Effect::Delay(ms) => {
                if !self.script_stepping {
                    self.status = "DELAY: no command script is running".into();
                } else if ms > 0 {
                    let until = self.script_clock.now() + Duration::from_millis(u64::from(ms));
                    if let Some(run) = &mut self.script {
                        run.phase = ScriptPhase::Delaying { until };
                    }
                }
                Ok(())
            }
            acad_cmd::Effect::Script(spec) => self.start_script(&spec),
            acad_cmd::Effect::Resume => {
                if self.script_stepping {
                    return Ok(());
                }
                match &mut self.script {
                    Some(run) if matches!(run.phase, ScriptPhase::Interrupted { .. }) => {
                        run.phase = ScriptPhase::Running;
                        self.status = format!("Resuming script at line {}", run.line);
                    }
                    _ => self.status = "RESUME: no interrupted command script".into(),
                }
                Ok(())
            }
            _ => unreachable!("only script effects are routed here"),
        }
    }
}
