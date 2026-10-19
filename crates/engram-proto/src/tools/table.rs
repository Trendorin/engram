//! The address table.

use super::{addr, dtype, obj, Access, ToolDef};
use serde_json::json;

pub(super) fn defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "table_list",
            summary: "The address table (saved cheats) with live values",
            description: "Entries the user or agent saved: id, description, address expression, type, live value, frozen. \
                The table is saved per game and reloaded on attach.",
            schema: obj(json!({}), &[]),
            access: Access::Read,
            example: "{}",
            group: "",
        },
        ToolDef {
            name: "table_add",
            summary: "Save an address to the table (visible and editable in the GUI)",
            description: "Adds an entry the user can see and edit. Use a clear description (\"Gold\", \"Player health\"). \
                Prefer a pointer-path expression from pointer_scan when available so it survives restarts.",
            schema: obj(
                json!({
                    "address": addr(), "type": dtype("i32"),
                    "description": { "type": "string" }
                }),
                &["address"],
            ),
            access: Access::Session,
            example: r#"{"address":"0x7f12a0","type":"i32","description":"Gold"}"#,
            group: "",
        },
        ToolDef {
            name: "table_set",
            summary: "Edit a table entry: value, freeze, description, address, type",
            description: "Setting `value` writes memory and `freeze`=true keeps rewriting the value (10x per second); \
                both need user approval when called by an agent. Changing `address` to a pointer path makes the entry durable.",
            schema: obj(
                json!({
                    "id": { "type": "integer" },
                    "value": { "type": "string" },
                    "freeze": { "type": "boolean" },
                    "description": { "type": "string" },
                    "address": addr(),
                    "type": { "type": "string" },
                    "reason": { "type": "string", "description": "Shown to the user in the approval prompt" }
                }),
                &["id"],
            ),
            access: Access::Mutate,
            example: r#"{"id":1,"freeze":true,"reason":"Infinite health"}"#,
            group: "",
        },
        ToolDef {
            name: "table_remove",
            summary: "Remove a table entry",
            description: "Deletes an entry (and its freeze).",
            schema: obj(json!({ "id": { "type": "integer" } }), &["id"]),
            access: Access::Session,
            example: r#"{"id":3}"#,
            group: "",
        },
    ]
}
