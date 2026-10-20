//! The single tool catalog. The daemon dispatches on these names, the MCP
//! server advertises them, `engram help` prints them and the docs are generated
//! from them. Descriptions are written for language models: say what it does,
//! when to use it and what comes next.
//!
//! Tools are grouped by domain, one file each. To add a tool: add a `ToolDef` to
//! the matching group (or a new group listed in `build`), then handle it in the
//! daemon's module of the same name (`engramd/src/tools/`).

mod agent;
mod analysis;
mod memory;
mod scan;
mod session;
mod table;

use serde_json::{json, Value};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Observes only.
    Read,
    /// Changes target memory or execution; the user approves agent calls in the GUI.
    Mutate,
    /// Changes Engram's own state (scan session, table, attachment).
    Session,
}

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: &'static str,
    pub summary: &'static str,
    pub description: &'static str,
    pub schema: Value,
    pub access: Access,
    pub example: &'static str,
    /// Domain group (see `GROUPS`); filled in by the catalog.
    pub group: &'static str,
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

const ADDR: &str = "Address expression: hex (0x7f12ab30), module+offset (Game.exe+0x1A2B30) or pointer path ([[Game.exe+0x10]+0x18]+0x40).";
const TYPES: &str = "i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 ptr bytes:N str:N str16:N";

/// Schema of an address argument.
fn addr() -> Value {
    json!({ "type": "string", "description": ADDR })
}

/// Schema of a data-type argument with default `d`.
fn dtype(d: &str) -> Value {
    json!({ "type": "string", "description": format!("Data type ({TYPES}). Default {d}."), "default": d })
}

pub fn catalog() -> &'static [ToolDef] {
    static CAT: OnceLock<Vec<ToolDef>> = OnceLock::new();
    CAT.get_or_init(build)
}

pub fn find(name: &str) -> Option<&'static ToolDef> {
    let name = name.strip_prefix("engram_").unwrap_or(name);
    catalog().iter().find(|t| t.name == name)
}

/// Groups in the order tools are listed (catalog, MCP, docs).
pub const GROUPS: &[(&str, &str)] = &[
    ("session", "Target and process"),
    ("scan", "Value scans"),
    ("table", "Address table"),
    ("memory", "Memory"),
    ("analysis", "Analysis"),
    ("agent", "Talking to the user"),
];

fn build() -> Vec<ToolDef> {
    let mut all = Vec::new();
    for (group, _) in GROUPS {
        let defs = match *group {
            "session" => session::defs(),
            "scan" => scan::defs(),
            "table" => table::defs(),
            "memory" => memory::defs(),
            "analysis" => analysis::defs(),
            "agent" => agent::defs(),
            other => unreachable!("tool group {other} has no definitions"),
        };
        all.extend(defs.into_iter().map(|mut d| {
            d.group = group;
            d
        }));
    }
    all
}

/// Methods the daemon accepts that are not agent tools.
pub const INTERNAL_METHODS: &[&str] =
    &["subscribe", "approval_respond", "policy", "launch_status", "activity", "reload_config", "shutdown", "hello"];
