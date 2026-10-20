//! Talking to the user.

use super::{obj, Access, ToolDef};
use serde_json::json;

pub(super) fn defs() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "notify",
        summary: "Show a message to the user in the Engram window",
        description: "Posts a message in the GUI activity feed (e.g. 'Found gold at Game.exe+0x1234, added to table').",
        schema: obj(
            json!({
                "message": { "type": "string" },
                "level": { "type": "string", "enum": ["info", "success", "warning"], "default": "info" }
            }),
            &["message"],
        ),
        access: Access::Read,
        example: r#"{"message":"Gold found and added to the table","level":"success"}"#,
        group: "",
    }]
}
