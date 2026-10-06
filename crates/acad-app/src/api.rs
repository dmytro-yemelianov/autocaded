//! Typed application operations shared by headless MCP and the GUI event loop.
use crate::{presentation::validate_frame_size, Session};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(
    tag = "method",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Request {
    New {},
    SetMode {
        id: String,
    },
    SetLocale {
        tag: String,
    },
    Open {
        path: PathBuf,
        #[serde(default)]
        directories: Vec<PathBuf>,
    },
    Command {
        input: String,
    },
    Point {
        x: f64,
        y: f64,
    },
    Click {
        x: f64,
        y: f64,
        width: Option<u32>,
        height: Option<u32>,
    },
    /// Pointer motion over a physical client pixel (SKETCH sampling).
    Motion {
        x: f64,
        y: f64,
        width: Option<u32>,
        height: Option<u32>,
    },
    State {},
    Drawing {},
    Save {
        path: String,
    },
    Cancel {},
    Report {
        action: crate::ReportAction,
        width: Option<u32>,
        height: Option<u32>,
    },
    Frame {
        width: Option<u32>,
        height: Option<u32>,
        #[serde(default)]
        format: FrameFormat,
    },
    Quit {
        #[serde(default)]
        discard: bool,
    },
    /// Start a command script (same lookup as the SCRIPT command).
    Script {
        path: String,
    },
    /// Read the script executor state without running it.
    ScriptStatus {},
    /// Run script work that is due on the session clock.
    ScriptTick {},
    /// Discard the current (running, delaying or interrupted) script.
    ScriptStop {},
    /// Show the Main Menu (clean drawings only); END/QUIT then return to it.
    MainMenu {},
}
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameFormat {
    #[default]
    Png,
    Rgba,
}

pub fn state(session: &Session) -> Value {
    let d = session.drawing();
    let h = &d.header;
    let sketch = session.editor.sketch_preview().map(|s| {
        json!({"pen_down":s.pen_down,"temporary_lines":s.temporary.len(),
            "mode":match s.mode { acad_cmd::SketchMode::Draw=>"draw", acad_cmd::SketchMode::Connect=>"connect", acad_cmd::SketchMode::Erase=>"erase" },
            "erase_from":s.erase_from})
    });
    json!({"mode":session.profile().definition().stable_id,"palette":session.palette().name(),"locale":match session.locale() { acad_cmd::messages::Locale::En => "en", acad_cmd::messages::Locale::Uk => "uk" }, "sketch":sketch,"command_idle":session.command_idle(),"prompt":session.prompt(),"status":session.status(),"input":session.input(),
        "report":session.report_text(),"report_view":{"visible":session.report_visible(),"anchor":session.report.as_ref().map(|r|r.anchor())},
        "entities":d.entities().count(),"selectable_objects":acad_cmd::selectable_items(d).count(),"blocks":d.blocks().count(),
        "path":session.document_path().map(|p|p.to_string_lossy()),"format":session.document_format(),"dirty":session.is_dirty(),
        "view":{"center":{"x":h.view.center.x,"y":h.view.center.y},"height":h.view.height},
        "limits":{"xmin":h.limits.xmin,"ymin":h.limits.ymin,"xmax":h.limits.xmax,"ymax":h.limits.ymax},
        "fillet_radius":h.fillet_radius,"current_layer":h.current_layer,"layers":h.layers,"off_layers":h.off_layers,
        "snap":{"on":h.snap.on,"spacing":h.snap.spacing},
        "grid":{"on":h.grid.on,"spacing":h.grid.spacing},"ortho":h.ortho,"script":session.script_status(),
        "main_menu":session.main_menu_state(),"returns_to_main_menu":session.returns_to_main_menu()})
}

/// Frame/click dimensions default to the attached window's current physical
/// client size. Headless clients use 800x600 unless they provide dimensions.
pub fn dispatch(
    session: &mut Session,
    request: Request,
    size: (u32, u32),
) -> Result<Value, String> {
    // Profile validation is atomic, including presentation status.
    if let Request::SetMode { id } = &request {
        acad_cmd::profiles::ProfileId::parse(id)?;
    }
    session.with_error_status(|session| perform(session, request, size))
}
fn perform(session: &mut Session, request: Request, size: (u32, u32)) -> Result<Value, String> {
    // Only editor-input requests and an explicit tick advance a command
    // script. Reads, report navigation, SAVE, NEW/OPEN/QUIT never run items.
    let advances_script = matches!(
        request,
        Request::Command { .. }
            | Request::Point { .. }
            | Request::Click { .. }
            | Request::Cancel {}
            | Request::Script { .. }
            | Request::ScriptTick {}
    );
    let quit = match request {
        Request::SetMode { id } => {
            session.set_mode(&id)?;
            false
        }
        Request::SetLocale { tag } => {
            session.set_locale_tag(&tag).map_err(|e| e.to_string())?;
            false
        }
        Request::ScriptStatus {} => return Ok(session.script_status()),
        Request::Script { path } => {
            session.interrupt_script(crate::ScriptInterrupt::Input);
            session.start_script(&path)?;
            false
        }
        Request::ScriptTick {} => false,
        Request::ScriptStop {} => {
            session.stop_script();
            false
        }
        Request::New {} => {
            session.reset();
            false
        }
        Request::Open { path, directories } => {
            let clock = session.script_clock();
            let mut opened = Session::open(&path, &directories)?;
            opened.inherit_main_menu_home(session);
            *session = opened;
            session.set_script_clock(clock);
            false
        }
        Request::MainMenu {} => {
            session.enter_main_menu()?;
            false
        }
        Request::Command { input } => {
            session.set_viewport_size(size.0, size.1)?;
            session.command_typed(&input)?
        }
        Request::Point { x, y } => {
            session.set_viewport_size(size.0, size.1)?;
            session.point(acad_model::Point { x, y })?
        }
        Request::Click {
            x,
            y,
            width,
            height,
        } => {
            let (w, h) = (width.unwrap_or(size.0), height.unwrap_or(size.1));
            validate_frame_size(w, h)?;
            session.click(x, y, w, h)?
        }
        Request::Motion {
            x,
            y,
            width,
            height,
        } => {
            let (w, h) = (width.unwrap_or(size.0), height.unwrap_or(size.1));
            validate_frame_size(w, h)?;
            session.motion(x, y, w, h)?
        }
        Request::State {} => return Ok(state(session)),
        Request::Drawing {} => {
            return Ok(
                json!({"format":"dxf","data":String::from_utf8(acad_dxf::try_write(session.drawing()).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?}),
            )
        }
        Request::Save { path } => {
            session.save(std::path::Path::new(&path))?;
            false
        }
        Request::Cancel {} => session.cancel()?,
        Request::Report {
            action,
            width,
            height,
        } => {
            let (width, height) = (width.unwrap_or(size.0), height.unwrap_or(size.1));
            validate_frame_size(width, height)?;
            session.report_action(action, width, height)?;
            false
        }
        Request::Frame {
            width,
            height,
            format,
        } => {
            let (width, height) = (width.unwrap_or(size.0), height.unwrap_or(size.1));
            validate_frame_size(width, height)?;
            let frame = session.frame(width, height)?;
            let (format, bytes) = match format {
                FrameFormat::Png => ("png", frame.png()?),
                FrameFormat::Rgba => ("rgba8", frame.rgba()),
            };
            return Ok(
                json!({"width":frame.width,"height":frame.height,"stride":frame.width*4,
                "format":format,"encoding":"base64","data":STANDARD.encode(bytes),"diagnostics":frame.diagnostics,"complete":frame.complete}),
            );
        }
        Request::Quit { discard: true } => true,
        Request::Quit { discard: false } => session.request_quit()?,
    };
    // Then run script work that is due (never waiting).
    let quit = quit || (advances_script && session.pump_script().quit);
    Ok(json!({"quit":quit,"state":state(session)}))
}
