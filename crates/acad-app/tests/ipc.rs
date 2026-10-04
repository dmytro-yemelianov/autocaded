#![cfg(unix)]
use acad_app::{api, ipc, Session};
use serde_json::json;
use std::{os::unix::fs::PermissionsExt, sync::mpsc, time::Duration};
#[test]
fn socket_delivers_requests_to_the_session_owner_and_cleans_up() {
    let path = std::env::temp_dir().join(format!("acad-ipc-{}.sock", std::process::id()));
    let (tx, rx) = mpsc::channel();
    let server = ipc::Server::bind(&path, move |event| {
        tx.send(event).map_err(|e| e.to_string())
    })
    .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let mut session = Session::default();
    for input in ["LINE", "1,2", "5,4", ""] {
        let path = path.clone();
        let request = json!({"method":"command","params":{"input":input}});
        let client = std::thread::spawn(move || ipc::call(&path, request));
        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let response = api::dispatch(&mut session, event.request, (640, 480));
        event.reply.send(response).unwrap();
        assert_eq!(
            client.join().unwrap().unwrap()["state"]["prompt"],
            session.prompt()
        );
    }
    assert_eq!(session.drawing().entities().count(), 1);
    let p = path.clone();
    let client = std::thread::spawn(move || ipc::call(&p, json!({"method":"state","params":{}})));
    let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    event
        .reply
        .send(api::dispatch(&mut session, event.request, (640, 480)))
        .unwrap();
    assert_eq!(client.join().unwrap().unwrap()["entities"], 1);
    // Large frame responses must not be truncated by fragmented JSON writes.
    let p = path.clone();
    let client = std::thread::spawn(move || {
        ipc::call(
            &p,
            json!({"method":"frame","params":{"width":160,"height":100,"format":"rgba"}}),
        )
    });
    let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    event
        .reply
        .send(api::dispatch(&mut session, event.request, (640, 480)))
        .unwrap();
    assert!(
        client.join().unwrap().unwrap()["data"]
            .as_str()
            .unwrap()
            .len()
            > 65536
    );
    drop(server);
    assert!(!path.exists());
}
#[test]
fn bind_never_removes_an_existing_file() {
    let path = std::env::temp_dir().join(format!("acad-ipc-existing-{}", std::process::id()));
    std::fs::write(&path, b"existing").unwrap();
    assert!(ipc::Server::bind(&path, |_| Ok(())).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"existing");
    std::fs::remove_file(path).unwrap();
}
