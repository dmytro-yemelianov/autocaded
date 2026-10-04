//! MCP stdio adapter pinned to the stable 2025-11-25 handshake protocol.
//! No editor/window ownership here; both backends dispatch the same typed API.
mod tools;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

#[derive(Default)]
pub struct Protocol {
    initialized: bool,
    ready: bool,
}
fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
impl Protocol {
    pub fn handle(
        &mut self,
        message: Value,
        call: &mut impl FnMut(Value) -> Result<Value, String>,
    ) -> Option<Value> {
        let id = message.get("id").cloned();
        if !message.is_object()
            || message.get("jsonrpc") != Some(&json!("2.0"))
            || !message.get("method").is_some_and(Value::is_string)
            || id
                .as_ref()
                .is_some_and(|v| !v.is_string() && !v.is_i64() && !v.is_u64())
        {
            return Some(error(Value::Null, -32600, "invalid JSON-RPC request"));
        }
        let method = message["method"].as_str().unwrap();
        if id.is_none() {
            if method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            return None;
        }
        let id = id.unwrap();
        let params = message.get("params").cloned().unwrap_or(json!({}));
        let result = match method {
            "initialize" if !self.initialized => {
                let Some(version) = params["protocolVersion"].as_str() else {
                    return Some(error(id, -32602, "protocolVersion must be a string"));
                };
                if !params["capabilities"].is_object()
                    || !params["clientInfo"]["name"].is_string()
                    || !params["clientInfo"]["version"].is_string()
                {
                    return Some(error(id, -32602, "invalid initialize parameters"));
                }
                let version = if ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"]
                    .contains(&version)
                {
                    version
                } else {
                    "2025-11-25"
                };
                self.initialized = true;
                json!({"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":"acad-rust","version":env!("CARGO_PKG_VERSION")},"instructions":"One shared drawing per process or attached native window. Submit one prompt input at a time; inspect acad_state. Capture frames directly without OS input synthesis."})
            }
            "initialize" => return Some(error(id, -32600, "already initialized")),
            "ping" => json!({}),
            _ if !self.ready => {
                return Some(error(
                    id,
                    -32000,
                    "initialize and send notifications/initialized first",
                ))
            }
            "tools/list" => {
                if !params.is_object() || params.get("cursor").is_some() {
                    return Some(error(id, -32602, "tool list has no pagination cursor"));
                }
                tools::list()
            }
            "tools/call" => {
                let Some(name) = params["name"].as_str() else {
                    return Some(error(id, -32602, "tool name must be a string"));
                };
                if !tools::NAMES.contains(&name) {
                    return Some(error(id, -32602, "unknown tool"));
                }
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                if !arguments.is_object() {
                    return Some(error(id, -32602, "arguments must be an object"));
                }
                match call(json!({"method":name.strip_prefix("acad_").unwrap(),"params":arguments}))
                {
                    Ok(mut value) => {
                        let mut content = Vec::new();
                        if value["format"] == "png" {
                            if let Some(data) = value.as_object_mut().and_then(|o| o.remove("data"))
                            {
                                content.push(
                                    json!({"type":"image","mimeType":"image/png","data":data}),
                                );
                            }
                        }
                        content.push(json!({"type":"text","text":value.to_string()}));
                        json!({"content":content,"structuredContent":value,"isError":false})
                    }
                    Err(e) => json!({"content":[{"type":"text","text":e}],"isError":true}),
                }
            }
            _ => return Some(error(id, -32601, "method not found")),
        };
        Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
}

pub fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    mut call: impl FnMut(Value) -> Result<Value, String>,
) -> Result<(), String> {
    let mut protocol = Protocol::default();
    while let Some(line) = crate::wire::read_message(&mut input)? {
        let response = match serde_json::from_str(&line) {
            Ok(value) => protocol.handle(value, &mut call),
            Err(_) => Some(error(Value::Null, -32700, "invalid JSON")),
        };
        if let Some(response) = response {
            crate::wire::write_message(&mut output, &response)?;
            output.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
