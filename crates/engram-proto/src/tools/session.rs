//! Target selection and process control.

use super::{obj, Access, ToolDef};
use serde_json::json;

pub(super) fn defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "status",
            summary: "Session overview: target, scan, table, permissions, next-step hints",
            description: "Call first in every session and whenever unsure. Returns the attached process (if any), \
                the current scan (types and candidate counts), the address table, the write-approval policy, \
                whether the GUI is open, and `hints` with suggested next steps.",
            schema: obj(json!({}), &[]),
            access: Access::Read,
            example: "{}",
            group: "",
        },
        ToolDef {
            name: "processes",
            summary: "List the user's processes (games first); Wine/Proton aware",
            description: "Lists processes owned by the user, largest memory first. Wine/Proton games appear under \
                their Windows name (Game.exe) with kind=proton and steam_app_id. Runtime helpers (wineserver, \
                services.exe, pressure-vessel, ...) are hidden unless include_helpers=true. Use `query` to filter \
                by name or command line, then attach by pid.",
            schema: obj(
                json!({
                    "query": { "type": "string", "description": "Case-insensitive substring of name/cmdline" },
                    "include_helpers": { "type": "boolean", "default": false },
                    "limit": { "type": "integer", "default": 25 }
                }),
                &[],
            ),
            access: Access::Read,
            example: r#"{"query":"minecraft"}"#,
            group: "",
        },
        ToolDef {
            name: "attach",
            summary: "Select the target process by pid or unique name",
            description: "Selects the process all other tools operate on. Engram does not stop the process. \
                Only processes of the same user can be attached. If several processes match `name`, the call \
                fails and lists them: ask the user which one. Loads the saved address table for this game.",
            schema: obj(
                json!({
                    "pid": { "type": "integer" },
                    "name": { "type": "string", "description": "Unique case-insensitive substring of the process name" }
                }),
                &[],
            ),
            access: Access::Session,
            example: r#"{"pid":12345}"#,
            group: "",
        },
        ToolDef {
            name: "detach",
            summary: "Forget the target (stops freezes, clears the scan)",
            description: "Releases the target. Frozen entries stop being written.",
            schema: obj(json!({}), &[]),
            access: Access::Session,
            example: "{}",
            group: "",
        },
        ToolDef {
            name: "process_control",
            summary: "Pause or resume the target (SIGSTOP/SIGCONT)",
            description: "Freezes the game for consistent multi-step edits. Always resume afterwards.",
            schema: obj(json!({ "action": { "type": "string", "enum": ["pause", "resume"] } }), &["action"]),
            access: Access::Mutate,
            example: r#"{"action":"pause"}"#,
            group: "",
        },
        ToolDef {
            name: "launch",
            summary: "Start a program as a child of Engram (traceable without extra privileges)",
            description: "Starts argv with Engram as its parent, which lets Engram read its memory even when the kernel \
                restricts ptrace (Yama) and Engram has no capability. Prefer asking the user; most setups use attach.",
            schema: obj(
                json!({
                    "argv": { "type": "array", "items": { "type": "string" } },
                    "cwd": { "type": "string" },
                    "env": { "type": "object", "additionalProperties": { "type": "string" } }
                }),
                &["argv"],
            ),
            access: Access::Mutate,
            example: r#"{"argv":["/usr/bin/supertuxkart"]}"#,
            group: "",
        },
    ]
}
