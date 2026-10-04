//! Opt-in Unix socket bridge. Only the GUI thread accesses the session.
use crate::api::Request;
use serde_json::{json, Value};
use std::{
    io::BufReader,
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

pub struct Event {
    pub request: Request,
    pub reply: mpsc::SyncSender<Result<Value, String>>,
}
pub struct Server {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
use crate::wire::read_message as read_request;

impl Server {
    pub fn bind(
        path: &Path,
        send: impl Fn(Event) -> Result<(), String> + Send + 'static,
    ) -> Result<Self, String> {
        let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            let _ = std::fs::remove_file(path);
            return Err(e.to_string());
        }
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // macOS accepts inherit the listener's nonblocking flag;
                        // frame writes would otherwise stop at the first full
                        // socket buffer. This worker uses explicit I/O timeouts.
                        if stream.set_nonblocking(false).is_err() {
                            continue;
                        }
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                        let result = (|| {
                            let text = read_request(&mut BufReader::new(&stream))?
                                .ok_or("empty request")?;
                            let request = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                            let (reply, response) = mpsc::sync_channel(1);
                            send(Event { request, reply })?;
                            response.recv_timeout(Duration::from_secs(30)).map_err(|e| format!("GUI response unavailable: {e}; do not automatically retry a mutation"))?
                        })();
                        let value = match result {
                            Ok(value) => json!({"result":value}),
                            Err(e) => json!({"error":e}),
                        };
                        let _ = crate::wire::write_message(&mut stream, &value);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            path: path.to_owned(),
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn call(path: &Path, request: Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(35)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    crate::wire::write_message(&mut stream, &request)?;
    let response: Value =
        serde_json::from_reader(BufReader::new(stream)).map_err(|e| e.to_string())?;
    if let Some(error) = response.get("error") {
        Err(error.as_str().unwrap_or("API error").to_owned())
    } else {
        response
            .get("result")
            .cloned()
            .ok_or_else(|| "missing API result".into())
    }
}
