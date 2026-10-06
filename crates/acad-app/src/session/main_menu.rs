//! The native Main Menu (docs/native-main-menu.md): the original's task list
//! and prompts, routed through the same `command` seam as the editor. The
//! Session owns every file the tasks read or write.
use super::*;
use crate::document::{write_without_backup, Format};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The original's eight tasks, in order (`crates/acad-oracle/tests/main_menu.rs`).
pub const MAIN_MENU_TASKS: [&str; 8] = [
    "Exit AutoCAD",
    static_label(acad_cmd::messages::MessageId::UiMainMenuNew),
    static_label(acad_cmd::messages::MessageId::UiMainMenuOpen),
    "Plot a drawing",
    "Configure AutoCAD",
    static_label(acad_cmd::messages::MessageId::UiMainMenuExportDxf),
    "Load drawing interchange file",
    "File Utilities",
];

const fn static_label(id: acad_cmd::messages::MessageId) -> &'static str {
    match acad_cmd::messages::text(id, acad_cmd::messages::Locale::En) {
        Ok(label) => label,
        Err(_) => panic!("static main menu label"),
    }
}

const CONFIGURATION_MENU: [&str; 8] = [
    "Exit to Main Menu",
    "Show current configuration",
    "Allow I/O port configuration",
    "Configure video display",
    "Configure digitizer",
    "Configure plotter",
    "Configure system console",
    "Configure operating parameters",
];

const RETURN_TO_MENU: &str = "Press RETURN to return to main menu";
const CONTINUE: &str = "Press RETURN to continue";
const INVALID: &str = "** Invalid data entered.";
const NO_DRAWING: &str = "** No drawing with this name is on file.";
const IMPROPER: &str = "Improper name for drawing.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Task {
    New,
    Edit,
    MakeDxf,
    LoadDxf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Selection,
    DrawingName(Task),
    Replace(PathBuf),
    Acknowledge,
    ConfigureShow,
    ConfigureMenu,
    ConfigureAcknowledge,
    /// The editor's FILES dialogue on a blank drawing; leaving it returns.
    FileUtilities,
}

pub(crate) struct MainMenu {
    screen: Screen,
    /// The dialogue lines shown under the menu (original wording).
    messages: Vec<String>,
}

impl MainMenu {
    fn at(screen: Screen) -> Self {
        Self {
            screen,
            messages: Vec::new(),
        }
    }
}

fn menu_lines(title: &str, items: &[&str]) -> String {
    let mut text = format!("{title}\n\n");
    for (number, item) in items.iter().enumerate() {
        text.push_str(&format!("  {number}.  {item}\n"));
        if number == 3 {
            text.push('\n');
        }
    }
    text
}

fn configuration() -> String {
    "Configure AutoCAD.\n\nCurrent AutoCAD configuration\n\n\
     \x20 Video display:  native window (host display, resizable)\n\
     \x20 Digitizer:      host pointer and keyboard\n\
     \x20 Plotter:        none (plotting is not implemented natively)\n"
        .into()
}

/// `Some(number)` for an all-digit selection; `None` is invalid data.
fn selection_number(line: &str) -> Option<u64> {
    let line = line.trim();
    if line.is_empty() || !line.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(line.parse().unwrap_or(u64::MAX))
}

impl Session {
    pub fn main_menu_tasks(&self) -> [&'static str; 8] {
        use acad_cmd::messages::{text, MessageId};
        let mut tasks = MAIN_MENU_TASKS;
        for (index, id) in [
            (1, MessageId::UiMainMenuNew),
            (2, MessageId::UiMainMenuOpen),
            (5, MessageId::UiMainMenuExportDxf),
        ] {
            tasks[index] = text(id, self.locale).expect("static main menu label");
        }
        tasks
    }
    /// Start at the Main Menu: `acad` launched without a drawing.
    pub fn main_menu(directories: &[PathBuf]) -> Self {
        let mut session = Self::new(directories);
        session.main_menu_home = true;
        session.main_menu = Some(MainMenu::at(Screen::Selection));
        session
    }

    /// API/MCP `main_menu`: close a clean drawing and show the Main Menu;
    /// from then on END and QUIT return to it. Unsaved changes are refused.
    pub fn enter_main_menu(&mut self) -> Result<(), String> {
        if self.main_menu.is_none() && self.is_dirty() {
            return Err(
                "Main Menu: the drawing has unsaved changes; END, SAVE or QUIT first".into(),
            );
        }
        self.main_menu_home = true;
        if self.main_menu.is_none() {
            self.return_to_main_menu();
        } else if self.in_file_utilities() {
            // Close the hosted FILES dialogue, exactly as Escape does.
            let _ = self.main_menu_cancel();
        } else {
            self.show_selection();
        }
        Ok(())
    }

    pub fn main_menu_active(&self) -> bool {
        self.main_menu.is_some()
    }

    /// True when END and QUIT return to the Main Menu.
    pub fn returns_to_main_menu(&self) -> bool {
        self.main_menu_home
    }

    pub(crate) fn in_file_utilities(&self) -> bool {
        self.main_menu
            .as_ref()
            .is_some_and(|menu| menu.screen == Screen::FileUtilities)
    }

    /// The Main Menu itself (not the FILES dialogue it hosts) takes input.
    pub(crate) fn main_menu_takes_input(&self) -> bool {
        self.main_menu.is_some() && !self.in_file_utilities()
    }

    pub(crate) fn refuse_at_main_menu(&self) -> Result<(), String> {
        if self.main_menu.is_some() {
            Err("Main Menu: no drawing is open; type a task number and press Return".into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn main_menu_prompt(&self) -> Option<&str> {
        let menu = self.main_menu.as_ref()?;
        Some(match &menu.screen {
            Screen::Selection | Screen::ConfigureMenu => "Enter selection",
            Screen::DrawingName(_) => match &self.main_menu_drawing {
                Some(_) => self.default_name_prompt.as_str(),
                None => "Enter NAME of drawing",
            },
            Screen::Replace(_) => "Do you want to replace it with the new drawing? <N>",
            Screen::Acknowledge => RETURN_TO_MENU,
            Screen::ConfigureShow | Screen::ConfigureAcknowledge => CONTINUE,
            Screen::FileUtilities => return None,
        })
    }

    /// The text screen the GUI paints and API `state` reports.
    pub fn main_menu_text(&self) -> Option<String> {
        let menu = self.main_menu.as_ref()?;
        let mut text = String::new();
        if let Some(name) = &self.main_menu_drawing {
            text.push_str(&format!("Current drawing:  {name}\n"));
        }
        match &menu.screen {
            Screen::ConfigureShow => text.push_str(&configuration()),
            Screen::ConfigureMenu | Screen::ConfigureAcknowledge => {
                text.push_str(&menu_lines("Configuration menu", &CONFIGURATION_MENU))
            }
            Screen::FileUtilities => text.push_str(
                "File Utilities\n\nThe File Utility Menu is the FILES report; \
                 selection 0 returns to the Main Menu.\n",
            ),
            _ => text.push_str(&menu_lines("Main Menu", &self.main_menu_tasks())),
        }
        if !menu.messages.is_empty() {
            text.push('\n');
            for message in &menu.messages {
                text.push_str(message);
                text.push('\n');
            }
        }
        Some(text)
    }

    /// API `state.main_menu`: `null` in the drawing editor.
    pub fn main_menu_state(&self) -> Value {
        let Some(menu) = &self.main_menu else {
            return Value::Null;
        };
        let (screen, task) = match &menu.screen {
            Screen::Selection => ("selection", None),
            Screen::DrawingName(task) => (
                "drawing_name",
                Some(match task {
                    Task::New => 1,
                    Task::Edit => 2,
                    Task::MakeDxf => 5,
                    Task::LoadDxf => 6,
                }),
            ),
            Screen::Replace(_) => ("replace", Some(1)),
            Screen::Acknowledge => ("acknowledge", None),
            Screen::ConfigureShow => ("configure_show", Some(4)),
            Screen::ConfigureMenu => ("configure_menu", Some(4)),
            Screen::ConfigureAcknowledge => ("configure_acknowledge", Some(4)),
            Screen::FileUtilities => ("file_utilities", Some(7)),
        };
        json!({
            "screen": screen,
            "task": task,
            "tasks": self.main_menu_tasks().iter().enumerate()
                .map(|(number, label)| json!({"number": number, "label": label}))
                .collect::<Vec<_>>(),
            "current_drawing": self.main_menu_drawing,
            "messages": menu.messages,
            "text": self.main_menu_text(),
        })
    }

    fn show(&mut self, screen: Screen, messages: Vec<String>) {
        if let Some(last) = messages.last() {
            self.set_status(last.clone());
        }
        self.main_menu = Some(MainMenu { screen, messages });
    }

    fn show_selection(&mut self) {
        self.report = None;
        self.show(Screen::Selection, Vec::new());
    }

    /// A finished dialogue: its lines, then "Press RETURN to return to main menu."
    fn acknowledge(&mut self, messages: Vec<String>, failed: bool) -> Result<bool, String> {
        let error = messages.last().cloned().unwrap_or_default();
        self.show(Screen::Acknowledge, messages);
        if failed {
            Err(error)
        } else {
            Ok(false)
        }
    }

    /// One Return-terminated Main Menu answer (window, API and MCP alike).
    pub(crate) fn main_menu_submit(&mut self, line: &str) -> Result<bool, String> {
        self.clear_status();
        let screen = self
            .main_menu
            .as_ref()
            .map(|menu| menu.screen.clone())
            .expect("Main Menu is shown");
        match screen {
            Screen::Selection => self.main_menu_select(line),
            Screen::DrawingName(task) => {
                let name = match line.trim() {
                    "" => match self.main_menu_drawing.clone() {
                        Some(name) => name,
                        None => return self.acknowledge(vec![IMPROPER.into()], true),
                    },
                    name => name.to_owned(),
                };
                // An improper name never becomes the current drawing.
                if let Err(error) = resolve(&name, "DWG") {
                    return self.acknowledge(vec![error], true);
                }
                self.main_menu_drawing = Some(name.clone());
                self.refresh_default_name_prompt();
                match task {
                    Task::New => self.task_new(&name),
                    Task::Edit => self.task_edit(&name),
                    Task::MakeDxf => self.task_make_dxf(&name),
                    Task::LoadDxf => self.task_load_dxf(&name),
                }
            }
            Screen::Replace(path) => {
                let answer = line.trim();
                if answer.eq_ignore_ascii_case("Y") || answer.eq_ignore_ascii_case("YES") {
                    self.begin_new_drawing(&path)
                } else {
                    self.show_selection();
                    Ok(false)
                }
            }
            Screen::Acknowledge => {
                self.show_selection();
                Ok(false)
            }
            Screen::ConfigureShow | Screen::ConfigureAcknowledge => {
                self.show(Screen::ConfigureMenu, Vec::new());
                Ok(false)
            }
            Screen::ConfigureMenu => match selection_number(line) {
                None => {
                    self.show(Screen::ConfigureAcknowledge, vec![INVALID.into()]);
                    Err(INVALID.into())
                }
                Some(0) => {
                    self.show_selection();
                    Ok(false)
                }
                Some(1) => {
                    self.show(Screen::ConfigureShow, Vec::new());
                    Ok(false)
                }
                Some(item @ 2..=7) => {
                    let message = format!(
                        "{}: not configurable natively; the host provides the display, \
                         pointer, plotter (none) and files.",
                        CONFIGURATION_MENU[item as usize]
                    );
                    self.show(Screen::ConfigureAcknowledge, vec![message.clone()]);
                    Err(message)
                }
                Some(_) => {
                    self.show(Screen::ConfigureMenu, Vec::new());
                    Ok(false)
                }
            },
            Screen::FileUtilities => unreachable!("FILES input goes to the editor"),
        }
    }

    fn main_menu_select(&mut self, line: &str) -> Result<bool, String> {
        match selection_number(line) {
            None => self.acknowledge(vec![INVALID.into()], true),
            Some(0) => {
                self.set_status("End AutoCAD.".into());
                Ok(true)
            }
            Some(1) => self.ask_name(Task::New),
            Some(2) => self.ask_name(Task::Edit),
            Some(3) => self.acknowledge(
                vec!["Plot a drawing: plotting is not available in the native application.".into()],
                true,
            ),
            Some(4) => {
                self.show(Screen::ConfigureShow, Vec::new());
                Ok(false)
            }
            Some(5) => self.ask_name(Task::MakeDxf),
            Some(6) => self.ask_name(Task::LoadDxf),
            Some(7) => {
                self.show(Screen::FileUtilities, Vec::new());
                let result = self.editor.submit_return("FILES");
                self.apply_effect(result)
            }
            // The original redisplays the menu for other numbers.
            Some(_) => {
                self.show_selection();
                Ok(false)
            }
        }
    }

    fn ask_name(&mut self, task: Task) -> Result<bool, String> {
        self.refresh_default_name_prompt();
        self.show(Screen::DrawingName(task), Vec::new());
        Ok(false)
    }

    fn refresh_default_name_prompt(&mut self) {
        self.default_name_prompt = match &self.main_menu_drawing {
            Some(name) => format!("Enter NAME of drawing (default {name})"),
            None => String::new(),
        };
    }

    fn task_new(&mut self, name: &str) -> Result<bool, String> {
        let path = match resolve(name, "DWG") {
            Ok(path) => path,
            Err(error) => return self.acknowledge(vec![error], true),
        };
        if std::fs::symlink_metadata(&path).is_ok() {
            self.show(
                Screen::Replace(path),
                vec!["** Warning!  A drawing with this name already exists.".into()],
            );
            return Ok(false);
        }
        self.begin_new_drawing(&path)
    }

    /// Task 1: a blank drawing attached to `path`; nothing is written until
    /// END, which backs up a replaced file like any save.
    fn begin_new_drawing(&mut self, path: &Path) -> Result<bool, String> {
        if !path.parent().is_some_and(Path::is_dir) {
            return self.acknowledge(
                vec![format!(
                    "{IMPROPER} ({}: no such directory)",
                    path.display()
                )],
                true,
            );
        }
        let (libraries, diagnostics) = Libraries::for_drawing(path, &self.directories);
        let mut next = Self::with_editor(acad_cmd::Editor::default(), libraries);
        next.document = Document::saved(path.to_path_buf(), Format::for_path(path), next.drawing());
        next.set_status(if diagnostics.is_empty() {
            format!("New drawing {}", path.display())
        } else {
            diagnostics.join("; ")
        });
        self.adopt(next);
        Ok(false)
    }

    fn task_edit(&mut self, name: &str) -> Result<bool, String> {
        let path = match resolve(name, "DWG") {
            Ok(path) if path.is_file() => path,
            Ok(_) => return self.acknowledge(vec![NO_DRAWING.into()], true),
            Err(error) => return self.acknowledge(vec![error], true),
        };
        match Self::open(&path, &self.directories) {
            Ok(next) => {
                self.adopt(next);
                Ok(false)
            }
            Err(error) => self.acknowledge(vec![format!("Cannot edit drawing: {error}")], true),
        }
    }

    fn task_make_dxf(&mut self, name: &str) -> Result<bool, String> {
        let source = match resolve(name, "DWG") {
            Ok(path) if path.is_file() => path,
            Ok(_) => return self.acknowledge(vec![NO_DRAWING.into()], true),
            Err(error) => return self.acknowledge(vec![error], true),
        };
        let target = sibling(&source, "DXF");
        let mut messages = vec!["Make drawing interchange file".to_owned()];
        if target == source {
            messages.push(format!(
                "{} is already a drawing interchange file.",
                source.display()
            ));
            return self.acknowledge(messages, true);
        }
        let written = std::fs::read(&source)
            .map_err(|e| format!("{}: {e}", source.display()))
            .and_then(|bytes| crate::document::decode_drawing(&bytes))
            .and_then(|drawing| write_without_backup(&target, Format::Dxf, &drawing));
        match written {
            Ok(warning) => {
                messages.push("Drawing interchange file complete.".into());
                messages.extend(warning.map(|w| format!("{}: {w}", target.display())));
                messages.push(format!("Wrote {}", target.display()));
                self.acknowledge(messages, false)
            }
            Err(error) => {
                messages.push(format!("Drawing interchange file not written: {error}"));
                self.acknowledge(messages, true)
            }
        }
    }

    fn task_load_dxf(&mut self, name: &str) -> Result<bool, String> {
        let source = match resolve(name, "DXF") {
            Ok(path) => path,
            Err(error) => return self.acknowledge(vec![error], true),
        };
        let target = if Path::new(name).extension().is_some() {
            sibling(&source, "DWG")
        } else {
            resolve(name, "DWG").unwrap_or_else(|_| sibling(&source, "DWG"))
        };
        let bytes = match std::fs::metadata(&source)
            .ok()
            .filter(std::fs::Metadata::is_file)
            .and_then(|_| std::fs::read(&source).ok())
        {
            Some(bytes) => bytes,
            None => {
                return self.acknowledge(
                    vec![format!("Could not open file {}", source.display())],
                    true,
                )
            }
        };
        let mut messages = vec!["Reading drawing interchange file".to_owned()];
        if target == source {
            messages.push(format!("{} cannot also be the drawing.", source.display()));
            return self.acknowledge(messages, true);
        }
        let loaded = match acad_dxf::parse(&bytes) {
            Ok(drawing) => drawing,
            Err(error) => {
                messages.push(format!("Format error in {}: {error}", source.display()));
                messages.push(format!("{} not written.", target.display()));
                return self.acknowledge(messages, true);
            }
        };
        let present = acad_dxf::lex(&bytes)
            .map(|records| records.into_iter().map(|record| record.keyword).collect())
            .unwrap_or_default();
        let replacing = std::fs::symlink_metadata(&target).is_ok();
        let merged = match merge_into_existing(&target, loaded, &present) {
            Ok(drawing) => drawing,
            Err(error) => {
                messages.push(error);
                messages.push(format!("{} not written.", target.display()));
                return self.acknowledge(messages, true);
            }
        };
        let format = Format::Dwg(acad_dwg::header::Version::Ac140);
        // Native safety extra (not original behaviour): an existing drawing
        // rewritten here keeps its previous bytes as its `.BAK`.
        let written = if replacing {
            format.save(&target, &merged)
        } else {
            write_without_backup(&target, format, &merged)
        };
        match written {
            Ok(warning) => {
                messages.push("End of drawing interchange file.".into());
                messages.extend(warning.map(|w| format!("{}: {w}", target.display())));
                messages.push(format!("Wrote {}", target.display()));
                self.acknowledge(messages, false)
            }
            Err(error) => {
                messages.push(format!("{} not written: {error}", target.display()));
                self.acknowledge(messages, true)
            }
        }
    }

    pub(crate) fn main_menu_cancel(&mut self) -> Result<bool, String> {
        if self.in_file_utilities() {
            let result = self.editor.cancel_command();
            let outcome = self.apply_effect(result);
            self.show_selection();
            return outcome;
        }
        self.input.clear();
        match self.main_menu.as_ref().map(|menu| &menu.screen) {
            Some(Screen::ConfigureAcknowledge) => self.show(Screen::ConfigureMenu, Vec::new()),
            _ => self.show_selection(),
        }
        Ok(false)
    }

    /// After a FILES submission: selection 0 (or cancel) left the dialogue.
    pub(crate) fn leave_finished_file_utilities(&mut self) {
        if self.in_file_utilities() && self.editor.command_idle() {
            self.show_selection();
        }
    }

    /// END/QUIT finished the drawing: `true` exits the process, otherwise
    /// the blank Main Menu replaces the drawing.
    pub(crate) fn leave_editor(&mut self) -> bool {
        if !self.main_menu_home || self.exit_requested {
            return true;
        }
        self.return_to_main_menu();
        false
    }

    fn return_to_main_menu(&mut self) {
        if let Some(path) = &self.document.path {
            self.main_menu_drawing = Some(path.display().to_string());
            self.refresh_default_name_prompt();
        }
        let status = std::mem::take(&mut self.status);
        let next = Self::new(&self.directories);
        self.adopt(next);
        self.main_menu = Some(MainMenu::at(Screen::Selection));
        self.set_status(status);
    }

    /// Replace the drawing session, keeping what belongs to the process.
    fn adopt(&mut self, mut next: Session) {
        next.script_clock = self.script_clock.clone();
        let (width, height) = self.viewport_size;
        next.set_viewport_size(width, height)
            .expect("retained viewport is positive");
        next.directories = std::mem::take(&mut self.directories);
        next.main_menu_home = self.main_menu_home;
        next.main_menu_drawing = self.main_menu_drawing.take();
        next.default_name_prompt = std::mem::take(&mut self.default_name_prompt);
        next.inherit_presentation(self);
        *self = next;
    }

    /// API `open` replaces the session but keeps the Main Menu policy and palette.
    pub fn inherit_main_menu_home(&mut self, previous: &Session) {
        self.main_menu_home = previous.main_menu_home;
        self.inherit_presentation(previous);
        self.main_menu_drawing = previous.main_menu_drawing.clone();
        self.refresh_default_name_prompt();
    }
}

/// Original task 6: the interchange file's records are appended to an
/// existing drawing of that name, and its header values replace the
/// drawing's. A missing drawing starts empty.
fn merge_into_existing(
    target: &Path,
    loaded: acad_model::Drawing,
    present: &std::collections::BTreeSet<String>,
) -> Result<acad_model::Drawing, String> {
    let mut existing = if std::fs::symlink_metadata(target).is_err() {
        // Native: a new drawing starts from the NEW-drawing defaults.
        acad_cmd::Editor::default().drawing().clone()
    } else {
        let bytes = std::fs::read(target).map_err(|e| format!("{}: {e}", target.display()))?;
        crate::document::decode_drawing(&bytes)
            .map_err(|e| format!("Cannot read existing drawing {}: {e}", target.display()))?
    };
    if let Some(block) = loaded
        .blocks()
        .find(|block| existing.block(&block.name).is_some())
    {
        return Err(format!(
            "Block {} is defined in both {} and the interchange file",
            block.name,
            target.display()
        ));
    }
    merge_header(&mut existing.header, &loaded.header, present);
    existing.items.extend(loaded.items);
    Ok(existing)
}

/// Only the header records the interchange file contains replace the
/// drawing's settings (original: an entities-only file keeps them all).
/// FILLET radius, units, axis and DWG passthrough bytes have no 1.4 DXF
/// record and are always kept.
fn merge_header(
    header: &mut acad_model::Header,
    loaded: &acad_model::Header,
    present: &std::collections::BTreeSet<String>,
) {
    for record in present {
        match record.as_str() {
            "EXTENTS" => header.extents = loaded.extents,
            "LIMITS" => header.limits = loaded.limits,
            "BASE" => header.base = loaded.base,
            "DWGVIEW" => header.view = loaded.view,
            "MODERES" => header.snap = loaded.snap,
            "MODEGRID" => header.grid = loaded.grid,
            "MODEORTHO" => header.ortho = loaded.ortho,
            "MODEFILL" => header.fill = loaded.fill,
            "TXTSIZE" => header.text_size = loaded.text_size,
            "TRACEWID" => header.trace_width = loaded.trace_width,
            "LAYER" => header.current_layer = loaded.current_layer,
            "LAYERC" => {
                header.layers = loaded.layers.clone();
                header.off_layers = loaded.off_layers.clone();
            }
            "DIMARROW" => header.dim_arrow = loaded.dim_arrow,
            _ => {}
        }
    }
}

/// A Main Menu drawing name as a host path: `X:NAME` uses the FILES drive
/// mapping, other names are relative to the current directory. A name
/// without an extension gets `extension`, preferring an existing file in
/// upper or lower case, else upper case.
pub(crate) fn resolve(name: &str, extension: &str) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("current directory: {e}"))?;
    resolve_in(name, extension, &cwd, &crate::files::drive_roots())
}

fn resolve_in(
    name: &str,
    extension: &str,
    cwd: &Path,
    drives: &std::collections::BTreeMap<char, PathBuf>,
) -> Result<PathBuf, String> {
    let name = name.trim();
    let bytes = name.as_bytes();
    let path = if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let rest = name[2..].trim_start_matches(['/', '\\']);
        if rest.is_empty() {
            return Err(IMPROPER.into());
        }
        crate::files::drive_path(bytes[0].to_ascii_uppercase() as char, cwd, drives)?.join(rest)
    } else {
        cwd.join(name)
    };
    if path.file_name().is_none() || name.ends_with(['/', '\\']) || name.contains(['*', '?']) {
        return Err(IMPROPER.into());
    }
    if path.extension().is_some() {
        return Ok(path);
    }
    Ok(sibling(&path, extension))
}

/// `path` with `extension`: the existing spelling (case matching `path`'s
/// own extension first), else that case.
fn sibling(path: &Path, extension: &str) -> PathBuf {
    let lower = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        e.bytes().any(|b| b.is_ascii_lowercase()) && !e.bytes().any(|b| b.is_ascii_uppercase())
    });
    let (first, second) = if lower {
        (
            extension.to_ascii_lowercase(),
            extension.to_ascii_uppercase(),
        )
    } else {
        (
            extension.to_ascii_uppercase(),
            extension.to_ascii_lowercase(),
        )
    };
    let preferred = path.with_extension(&first);
    if std::fs::symlink_metadata(&preferred).is_ok() {
        return preferred;
    }
    let other = path.with_extension(&second);
    if std::fs::symlink_metadata(&other).is_ok() {
        return other;
    }
    preferred
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_take_the_task_extension_and_drive_mapping() {
        let cwd = std::env::temp_dir();
        let drives = std::collections::BTreeMap::from([('B', PathBuf::from("/drive-b"))]);
        assert_eq!(
            resolve_in("D2", "DWG", &cwd, &drives).unwrap(),
            cwd.join("D2.DWG")
        );
        assert_eq!(
            resolve_in("B:plan.dxf", "DWG", &cwd, &drives).unwrap(),
            PathBuf::from("/drive-b/plan.dxf")
        );
        for improper in ["dir/", "*", "A:"] {
            assert!(
                resolve_in(improper, "DWG", &cwd, &drives).is_err(),
                "{improper}"
            );
        }
        assert!(resolve_in("C:X", "DWG", &cwd, &drives)
            .unwrap_err()
            .contains("no directory mapping"));
    }

    #[test]
    fn selections_are_digits_only() {
        assert_eq!(selection_number(" 7 "), Some(7));
        assert_eq!(selection_number("12"), Some(12));
        assert_eq!(selection_number(""), None);
        assert_eq!(selection_number("X"), None);
        assert_eq!(selection_number("-1"), None);
    }

    #[test]
    fn a_macro_end_returns_to_the_menu_and_drops_the_remaining_steps() {
        let dir = std::env::temp_dir().join(format!("acad-menu-macro-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let mut s = Session::main_menu(&[]);
        s.command("1").unwrap();
        s.command(dir.join("M").to_str().unwrap()).unwrap();
        assert_eq!(s.prompt(), "Command");
        s.run_macro("END LINE", &mut |session, result| {
            session.apply_effect(result).unwrap();
        });
        assert!(s.main_menu_active());
        assert_eq!(
            s.main_menu_state()["screen"],
            "selection",
            "LINE never reached the menu"
        );
        assert_eq!(
            s.editor.prompt(),
            "Command",
            "LINE never reached the blank editor"
        );
        assert!(dir.join("M.DWG").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_new_drawing_in_a_missing_directory_is_reported() {
        let mut s = Session::main_menu(&[]);
        s.command("1").unwrap();
        let missing = std::env::temp_dir()
            .join("acad-no-such-dir-for-main-menu")
            .join("D2");
        let error = s.command(missing.to_str().unwrap()).unwrap_err();
        assert!(error.contains("no such directory"), "{error}");
        assert_eq!(s.prompt(), RETURN_TO_MENU);
    }
}
