//! Shared between the daemon, the GUI, the CLI and the MCP server.
//!
//! Wire format: one JSON object per line over a Unix socket.
//! * request  `{"id":1,"method":"scan","params":{...},"client":"mcp:opencode"}`
//! * response `{"id":1,"ok":true,"result":{...}}` / `{"id":1,"ok":false,"error":{...}}`
//! * after a `subscribe` request the connection only carries events:
//!   `{"event":"activity","data":{...}}`

pub mod agentkit;
pub mod client;
pub mod config;
pub mod paths;
pub mod tools;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl RpcError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), hint: None }
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(h) = &self.hint {
            write!(f, " (hint: {h})")?;
        }
        Ok(())
    }
}

impl std::error::Error for RpcError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    #[serde(default)]
    pub data: Value,
}

/// Best-effort name for whoever runs the CLI, shown in the GUI activity feed.
pub fn detect_client() -> String {
    if let Ok(c) = std::env::var("ENGRAM_CLIENT") {
        return c;
    }
    let env = |k: &str| std::env::var_os(k).is_some();
    if env("CLAUDECODE") || env("CLAUDE_CODE_ENTRYPOINT") {
        "claude-code".into()
    } else if env("OPENCODE") || env("OPENCODE_BIN_PATH") {
        "opencode".into()
    } else if env("CODEX_SANDBOX") || env("CODEX_HOME") {
        "codex".into()
    } else if env("GEMINI_CLI") {
        "gemini".into()
    } else {
        "cli".into()
    }
}
