//! Memory layout, reading and writing.

use super::{addr, dtype, obj, Access, ToolDef};
use serde_json::json;

pub(super) fn defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "modules",
            summary: "Loaded modules (ELF .so / PE .exe/.dll) with base addresses",
            description: "Modules of the target with base and size. For Proton games the game's .exe and .dll files appear \
                here; use their names in address expressions (Game.exe+0x1234).",
            schema: obj(
                json!({
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "default": 40 }
                }),
                &[],
            ),
            access: Access::Read,
            example: r#"{"query":".exe"}"#,
            group: "",
        },
        ToolDef {
            name: "regions",
            summary: "Memory map summary (heap/stack/anon/image regions)",
            description: "Memory regions of the target with permissions and kind. Mostly useful to understand where an \
                address lives; `analyze` already explains single addresses.",
            schema: obj(
                json!({
                    "writable_only": { "type": "boolean", "default": false },
                    "kind": { "type": "string", "enum": ["heap", "stack", "anon", "image", "special"] },
                    "limit": { "type": "integer", "default": 50 }
                }),
                &[],
            ),
            access: Access::Read,
            example: r#"{"writable_only":true,"limit":20}"#,
            group: "",
        },
        ToolDef {
            name: "read",
            summary: "Read typed values at an address",
            description: "Reads `count` consecutive values of `type` starting at `address`. Works while the game runs.",
            schema: obj(
                json!({
                    "address": addr(), "type": dtype("i32"),
                    "count": { "type": "integer", "default": 1, "maximum": 256 }
                }),
                &["address"],
            ),
            access: Access::Read,
            example: r#"{"address":"0x7f1234560010","type":"f32","count":3}"#,
            group: "",
        },
        ToolDef {
            name: "hexdump",
            summary: "Hex + ASCII dump of memory",
            description: "Classic 16-bytes-per-line dump. Prefer `dissect` for understanding structures.",
            schema: obj(json!({ "address": addr(), "size": { "type": "integer", "default": 256, "maximum": 4096 } }), &["address"]),
            access: Access::Read,
            example: r#"{"address":"Game.exe+0x1000","size":64}"#,
            group: "",
        },
        ToolDef {
            name: "write",
            summary: "Write a typed value (needs user approval for agents)",
            description: "Writes `value` as `type` at `address`. ALWAYS pass `expected` (the value you just read) so a \
                stale address is rejected instead of corrupting memory, and a human-readable `reason` the user sees in \
                the approval prompt. Read the value back afterwards and ask the user to confirm the in-game effect.",
            schema: obj(
                json!({
                    "address": addr(), "type": dtype("i32"),
                    "value": { "type": "string", "description": "New value, formatted for the type (\"200\", \"1.5\", \"90 90\", \"text\")" },
                    "expected": { "type": "string", "description": "Current value; the write is refused if memory differs" },
                    "reason": { "type": "string", "description": "Shown to the user, e.g. 'Double gold: 150 -> 300'" }
                }),
                &["address", "value"],
            ),
            access: Access::Mutate,
            example: r#"{"address":"0x7f12a0","type":"i32","value":"300","expected":"150","reason":"Double gold"}"#,
            group: "",
        },
    ]
}
