use std::io::{self, BufRead, Write};
use clared_core::{EpochSession, JsonRpcRequest, JsonRpcResponse, JsonRpcError, PolicyEngine};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("🛡️  Clared Guard (DTBE Proxy) v0.1.0 starting...");
    eprintln!("    • Mode: In-Process Decision Graph + Egress Escrow");
    eprintln!("    • Wire Protocol: Model Context Protocol (MCP) JSON-RPC 2.0");

    let policy_engine = PolicyEngine::new("default_policy_bundle");
    let session = EpochSession::new("acme_default");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        // Parse MCP JSON-RPC Request
        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(&line) {
            if req.method == "tools/call" {
                if let Some(params) = req.params {
                    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                    // Process through Clared Escrow
                    match session.intercept_tool_call(tool_name, &arguments, &policy_engine) {
                        Ok(escrow_result) => {
                            let resp = JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: req.id,
                                result: Some(json!({
                                    "content": [{
                                        "type": "text",
                                        "text": escrow_result.to_string()
                                    }]
                                })),
                                error: None,
                            };
                            let out = serde_json::to_string(&resp)?;
                            writeln!(stdout, "{}", out)?;
                            stdout.flush()?;
                        }
                        Err(err_msg) => {
                            let resp = JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: req.id,
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32001,
                                    message: format!("CLRED_INVARIANT_BREACH: {}", err_msg),
                                    data: Some(json!({ "epoch_id": session.epoch_id })),
                                }),
                            };
                            let out = serde_json::to_string(&resp)?;
                            writeln!(stdout, "{}", out)?;
                            stdout.flush()?;
                        }
                    }
                }
            } else {
                // Pass-through other MCP methods (e.g. tools/list, initialize)
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: Some(json!({ "status": "PASSTHROUGH_ACK" })),
                    error: None,
                };
                let out = serde_json::to_string(&resp)?;
                writeln!(stdout, "{}", out)?;
                stdout.flush()?;
            }
        }
    }

    Ok(())
}
