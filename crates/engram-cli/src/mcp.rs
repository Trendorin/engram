//! Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0).
//! Every Engram tool becomes an MCP tool; calls are forwarded to engramd.

use engram_proto::agentkit::MCP_INSTRUCTIONS;
use engram_proto::client::Client;
use engram_proto::tools::{self, Access};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::process::ExitCode;

const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Tools agents get over MCP. `launch` stays with the user (window, `engram launch`).
fn exposed(name: &str) -> bool {
    name != "launch"
}

fn tool_list() -> Value {
    let list: Vec<Value> = tools::catalog()
        .iter()
        .filter(|t| exposed(t.name))
        .map(|t| {
            json!({
                "name": t.name,
                "title": t.summary,
                "description": t.description,
                "inputSchema": t.schema,
                "annotations": {
                    "readOnlyHint": t.access == Access::Read,
                    "destructiveHint": t.access == Access::Mutate,
                    "idempotentHint": false,
                    "openWorldHint": false,
                },
            })
        })
        .collect();
    json!({ "tools": list })
}

pub fn run() -> ExitCode {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let mut client: Option<Client> = None;
    let mut name = "mcp".to_string();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                reply(&mut out, json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } }));
                continue;
            }
        };
        let Some(id) = msg.get("id").cloned() else { continue }; // notification
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let result: Result<Value, (i64, String)> = match method {
            "initialize" => {
                if let Some(n) = params.pointer("/clientInfo/name").and_then(|v| v.as_str()) {
                    name = format!("mcp:{n}");
                }
                let asked = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(SUPPORTED[0]);
                let version = if SUPPORTED.contains(&asked) { asked } else { SUPPORTED[0] };
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "engram", "title": "Engram memory scanner", "version": engram_proto::VERSION },
                    "instructions": MCP_INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(tool_list()),
            "resources/list" => Ok(json!({ "resources": [] })),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            "tools/call" => Ok(call(&mut client, &name, &params)),
            other => Err((-32601, format!("method not found: {other}"))),
        };
        let resp = match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
        };
        reply(&mut out, resp);
    }
    ExitCode::SUCCESS
}

fn reply(out: &mut impl Write, v: Value) {
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn call(client: &mut Option<Client>, name: &str, params: &Value) -> Value {
    let tool = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let text_result = |v: &Value, is_error: bool| json!({ "content": [{ "type": "text", "text": v.to_string() }], "isError": is_error });
    let Some(def) = tools::find(tool).filter(|d| exposed(d.name)) else {
        return text_result(&json!({ "error": { "code": "unknown_tool", "message": format!("no tool '{tool}'") } }), true);
    };
    // One retry with a fresh connection covers a daemon restart between calls.
    for attempt in 0..2 {
        if client.is_none() {
            match Client::connect_or_spawn(name) {
                Ok(c) => *client = Some(c),
                Err(e) => return text_result(&json!({ "error": e }), true),
            }
        }
        match client.as_mut().unwrap().call(def.name, args.clone()) {
            Ok(v) => return text_result(&v, false),
            Err(e) if e.code == "daemon_unavailable" && attempt == 0 => *client = None,
            Err(e) => return text_result(&json!({ "error": e }), true),
        }
    }
    text_result(&json!({ "error": { "code": "daemon_unavailable" } }), true)
}
