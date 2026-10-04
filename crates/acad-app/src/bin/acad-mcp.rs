//! Launch directly as an MCP stdio server, or proxy an existing native window.
use acad_app::{api, mcp, Session};
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut socket = None;
    let mut drawing = None;
    let mut directories = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--socket" => {
                socket = Some(std::path::PathBuf::from(
                    args.next().ok_or("--socket requires a path")?,
                ))
            }
            "--drawing" => {
                drawing = Some(std::path::PathBuf::from(
                    args.next().ok_or("--drawing requires a path")?,
                ))
            }
            "--fonts" => directories.push(std::path::PathBuf::from(
                args.next().ok_or("--fonts requires a directory")?,
            )),
            "--help" => {
                eprintln!("acad-mcp [--drawing PATH] [--fonts DIR] | --socket PATH");
                return Ok(());
            }
            _ => return Err(format!("unknown option: {arg}")),
        }
    }
    if let Some(socket) = socket {
        if drawing.is_some() || !directories.is_empty() {
            return Err("--socket cannot be combined with headless drawing/font options".into());
        }
        #[cfg(unix)]
        return mcp::serve(
            std::io::stdin().lock(),
            std::io::stdout().lock(),
            move |request| acad_app::ipc::call(&socket, request),
        );
        #[cfg(not(unix))]
        return Err(format!("GUI sockets require Unix: {}", socket.display()));
    }
    let mut session = if let Some(path) = drawing {
        Session::open(&path, &directories)?
    } else {
        Session::new(&directories)
    };
    let mut quit = false;
    mcp::serve(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        move |value| {
            if quit {
                return Err("session has exited".into());
            }
            let request = serde_json::from_value(value).map_err(|e| e.to_string())?;
            let result = api::dispatch(&mut session, request, (800, 600))?;
            quit = result["quit"].as_bool().unwrap_or(false);
            Ok(result)
        },
    )
}
fn main() {
    if let Err(e) = run() {
        eprintln!("acad-mcp: {e}");
        std::process::exit(2);
    }
}
