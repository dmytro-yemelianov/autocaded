mod bitmap;
mod command_line;
mod document;
mod files;
mod grid;
mod menu_panel;
mod presentation;
mod report_view;
mod session;
mod text_metrics;
pub use document::decode_drawing;
pub use presentation::Frame;
pub use report_view::ReportAction;
pub use session::{
    KeyModifiers, ScriptClock, ScriptInterrupt, ScriptPhase, ScriptPump, Session, SystemClock,
    MAIN_MENU_TASKS, MAX_SCRIPT_BYTES, SCRIPT_ITEMS_PER_PUMP,
};
pub mod api;
#[cfg(unix)]
pub mod ipc;
pub mod mcp;
mod wire;

#[cfg(test)]
/// A retained corpus file read at run time (the corpus is not in git). Absent
/// files skip the calling test visibly; `AUTOCAD_REQUIRE_CORPUS=1` makes
/// absence a failure.
pub(crate) fn corpus_file(path: &str) -> Option<Vec<u8>> {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(path);
    match std::fs::read(&full) {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            let message = format!("corpus file {} absent: {error}", full.display());
            assert!(
                std::env::var_os("AUTOCAD_REQUIRE_CORPUS").is_none(),
                "{message}"
            );
            eprintln!("skipping corpus test, NOT validated: {message}");
            None
        }
    }
}
