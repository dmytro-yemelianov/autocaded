use acad_app::{api, mcp::Protocol, Session};
use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn initialize() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
}
#[test]
fn lifecycle_and_protocol_errors_do_not_execute_tools() {
    let mut protocol = Protocol::default();
    let mut calls = 0;
    let mut backend = |_: Value| {
        calls += 1;
        Ok(json!({}))
    };
    let early = protocol
        .handle(
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            &mut backend,
        )
        .unwrap();
    assert!(early.get("error").is_some());
    assert_eq!(
        protocol.handle(initialize(), &mut backend).unwrap()["result"]["protocolVersion"],
        "2025-11-25"
    );
    assert!(protocol
        .handle(
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            &mut backend
        )
        .is_none());
    let tools = protocol
        .handle(
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
            &mut backend,
        )
        .unwrap();
    let listed = tools["result"]["tools"].as_array().unwrap();
    assert_eq!(listed.len(), 17);
    let script_tools: Vec<_> = listed
        .iter()
        .filter(|tool| tool["name"].as_str().unwrap().starts_with("acad_script"))
        .map(|tool| {
            (
                tool["name"].as_str().unwrap(),
                tool["annotations"]["readOnlyHint"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        script_tools,
        [
            ("acad_script", false),
            ("acad_script_status", true),
            ("acad_script_tick", false),
            ("acad_script_stop", false)
        ]
    );
    for (method, params, code) in [
        ("unknown", json!({}), -32601),
        ("tools/call", json!({"name":"missing"}), -32602),
        (
            "tools/call",
            json!({"name":"acad_command","arguments":[]}),
            -32602,
        ),
    ] {
        assert_eq!(
            protocol
                .handle(
                    json!({"jsonrpc":"2.0","id":"request","method":method,"params":params}),
                    &mut backend
                )
                .unwrap()["error"]["code"],
            code
        );
    }
    assert_eq!(calls, 0);
}
#[test]
fn png_tool_returns_image_without_duplicating_pixels_in_metadata() {
    let mut protocol = Protocol::default();
    let mut session = Session::default();
    let mut backend = |value| {
        api::dispatch(
            &mut session,
            serde_json::from_value(value).map_err(|e| e.to_string())?,
            (640, 480),
        )
    };
    protocol.handle(initialize(), &mut backend);
    protocol.handle(
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        &mut backend,
    );
    let response = protocol
        .handle(
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"acad_frame"}}),
            &mut backend,
        )
        .unwrap();
    assert_eq!(response["result"]["content"][0]["type"], "image");
    assert_eq!(response["result"]["content"][0]["mimeType"], "image/png");
    assert_eq!(response["result"]["structuredContent"]["width"], 640);
    assert!(response["result"]["structuredContent"]
        .get("data")
        .is_none());
}

#[test]
fn report_tool_uses_the_shared_session_and_rejects_invalid_navigation() {
    let mut protocol = Protocol::default();
    let mut session = Session::default();
    for input in ["HELP", "ZOOM"] {
        session.command(input).unwrap();
    }
    let before = session.drawing().clone();
    let mut backend = |value| {
        api::dispatch(
            &mut session,
            serde_json::from_value(value).map_err(|e| e.to_string())?,
            (640, 240),
        )
    };
    protocol.handle(initialize(), &mut backend);
    protocol.handle(
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        &mut backend,
    );
    for (action, visible, is_error) in [
        ("end", true, false),
        ("close", false, false),
        ("open", true, false),
        ("invalid", true, true),
    ] {
        let response = protocol.handle(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"acad_report","arguments":{"action":action}}}),&mut backend).unwrap();
        assert_eq!(response["result"]["isError"], is_error);
        if !is_error {
            assert_eq!(
                response["result"]["structuredContent"]["state"]["report_view"]["visible"],
                visible
            );
        }
    }
    assert_eq!(session.drawing(), &before);
    assert!(!session.is_dirty());
}
#[test]
fn real_stdio_binary_keeps_reports_off_stdout_and_recovers_after_bad_input() {
    let mut messages = vec![
        initialize(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ];
    for (id, name, args) in [
        (2, "acad_command", json!({"input":"LINE"})),
        (3, "acad_point", json!({"x":1,"y":2})),
        (4, "acad_point", json!({"x":5,"y":4})),
        (5, "acad_command", json!({"input":""})),
        (6, "acad_command", json!({"input":"DBLIST"})),
        (7, "acad_state", json!({})),
        (8, "acad_command", json!({"input":"unknown"})),
        (
            9,
            "acad_frame",
            json!({"width":160,"height":100,"format":"rgba"}),
        ),
    ] {
        messages.push(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}}));
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_acad-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "invalid json").unwrap();
    for message in messages {
        writeln!(stdin, "{message}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 10);
    assert_eq!(responses[0]["error"]["code"], -32700);
    let state = responses.iter().find(|v| v["id"] == 7).unwrap();
    assert_eq!(state["result"]["structuredContent"]["entities"], 1);
    assert!(state["result"]["structuredContent"]["report"]
        .as_str()
        .unwrap()
        .contains("Entity 1: Line"));
    assert_eq!(
        responses.iter().find(|v| v["id"] == 8).unwrap()["result"]["isError"],
        true
    );
    assert_eq!(
        responses.iter().find(|v| v["id"] == 9).unwrap()["result"]["structuredContent"]["format"],
        "rgba8"
    );
}

#[test]
fn real_stdio_quit_confirmation_then_end_saves_before_the_session_exits() {
    let root = std::env::temp_dir().join(format!("acad-mcp-end-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let _cleanup = Cleanup(root.clone());
    let path = root.join("saved drawing.dwg");
    let mut messages = vec![
        initialize(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ];
    for (id, name, arguments) in [
        (2, "acad_command", json!({"input":"POINT"})),
        (3, "acad_point", json!({"x":3,"y":4})),
        (4, "acad_quit", json!({})),
        (5, "acad_command", json!({"input":"N"})),
        (6, "acad_state", json!({})),
        (7, "acad_command", json!({"input":"END"})),
        (8, "acad_command", json!({"input":path})),
        (9, "acad_state", json!({})),
    ] {
        messages.push(json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}}));
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_acad-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for message in messages {
        writeln!(stdin, "{message}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let result = |id| responses.iter().find(|v| v["id"] == id).unwrap()["result"].clone();
    assert_eq!(result(4)["structuredContent"]["quit"], false);
    assert!(result(4)["structuredContent"]["state"]["prompt"]
        .as_str()
        .unwrap()
        .starts_with("QUIT:"));
    assert_eq!(result(6)["structuredContent"]["prompt"], "Command");
    assert_eq!(result(6)["structuredContent"]["dirty"], true);
    assert_eq!(result(7)["structuredContent"]["quit"], false);
    assert_eq!(result(8)["structuredContent"]["quit"], true);
    assert_eq!(result(8)["structuredContent"]["state"]["dirty"], false);
    assert_eq!(result(8)["structuredContent"]["state"]["path"], json!(path));
    assert_eq!(result(9)["isError"], true);
    assert_eq!(
        Session::open(&path, &[])
            .unwrap()
            .drawing()
            .entities()
            .count(),
        1
    );
}

#[test]
fn selected_list_mcp_collects_deduplicated_points_and_reports_without_mutation() {
    let mut protocol = Protocol::default();
    let mut session = Session::default();
    for input in ["POINT", "1,1", "POINT", "3,3", "POINT", "5,5"] {
        session.command(input).unwrap();
    }
    let before = session.drawing().clone();
    let dirty = session.is_dirty();
    let mut backend = |value| {
        api::dispatch(
            &mut session,
            serde_json::from_value(value).map_err(|e| e.to_string())?,
            (640, 240),
        )
    };
    protocol.handle(initialize(), &mut backend);
    protocol.handle(
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        &mut backend,
    );
    let mut call = |name, arguments| {
        protocol.handle(
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
        &mut backend).unwrap()
    };
    let start = call("acad_command", json!({"input":"LIST"}));
    assert!(start["result"]["structuredContent"]["state"]["prompt"]
        .as_str()
        .unwrap()
        .starts_with("LIST:"));
    for (x, y) in [(1, 1), (3, 3), (1, 1)] {
        let response = call("acad_point", json!({"x":x,"y":y}));
        assert_eq!(response["result"]["isError"], false);
    }
    let response = call("acad_command", json!({"input":""}));
    let state = &response["result"]["structuredContent"]["state"];
    assert_eq!(state["prompt"], "Command");
    assert!(state["report"]
        .as_str()
        .unwrap()
        .starts_with("1 POINT, 2 POINT\n"));
    assert!(!state["report"].as_str().unwrap().contains("3: layer="));
    for action in ["end", "home", "close", "open"] {
        let response = call("acad_report", json!({"action":action}));
        assert_eq!(response["result"]["isError"], false);
        assert_eq!(
            response["result"]["structuredContent"]["state"]["report"],
            state["report"]
        );
    }
    assert_eq!(session.drawing(), &before);
    assert_eq!(session.is_dirty(), dirty);
}
