//! Echo plugin — demo for P3-4.
//!
//! A minimal MCP server with the `uclaw` capability extension. Echoes
//! whatever you send it. Used to verify the plugin discovery → registration
//! → tool dispatch path end-to-end.
//!
//! Plugin manifest at `src-tauri/examples/echo_plugin/plugin.toml` declares
//! this binary as the runtime. Build with `cargo build --example echo_plugin`.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("echo_plugin: bad input: {}", e);
                continue;
            }
        };
        let id = req.id.unwrap_or(serde_json::Value::Null);
        let response = match req.method.as_str() {
            "initialize" => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "uclaw": {
                            "version": "1.0",
                            "hooks": [],
                            "renderers": []
                        }
                    },
                    "serverInfo": {
                        "name": "echo_plugin",
                        "version": "0.1.0"
                    }
                })),
                error: None,
            },
            "tools/list" => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(serde_json::json!({
                    "tools": [
                        {
                            "name": "echo",
                            "description": "Echoes the input back",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "message": { "type": "string" }
                                },
                                "required": ["message"]
                            }
                        }
                    ]
                })),
                error: None,
            },
            "tools/call" => {
                let message = req
                    .params
                    .get("arguments")
                    .and_then(|a| a.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("(no message)");
                JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(serde_json::json!({
                        "content": [
                            { "type": "text", "text": message }
                        ]
                    })),
                    error: None,
                }
            }
            other => JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("method {} not found", other),
                }),
            },
        };
        let line = serde_json::to_string(&response).unwrap();
        writeln!(out, "{}", line)?;
        out.flush()?;
    }
    Ok(())
}
