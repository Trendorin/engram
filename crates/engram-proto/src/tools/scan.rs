//! Value scanning.

use super::{obj, Access, ToolDef};
use serde_json::json;

pub(super) fn defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "scan",
            summary: "Start a new value scan (exact/unknown/range); type=auto tries several types",
            description: "Starts a NEW scan (replacing the previous one) over writable memory. \
                compare: exact | unknown | greater | less | between | not_equal. \
                type: auto (default: tries i32,i64,f32,f64 at once and ranks candidates), a numeric type, or \
                bytes (array-of-bytes pattern with ?? wildcards), string, string16 (UTF-16, common in Unity/.NET). \
                Floats match with rounding to the digits given (\"100\" matches 99.5..100.5). \
                Typical flow: scan value=<number the game shows> -> ask the user to change it in-game -> scan_next. \
                Returns counts per type and the best-ranked candidates.",
            schema: obj(
                json!({
                    "value": { "type": "string", "description": "Value to look for (not needed for unknown)" },
                    "value2": { "type": "string", "description": "Upper bound for between" },
                    "type": { "type": "string", "default": "auto", "description": "auto | i8..i64 | u8..u64 | f32 | f64 | bytes | string | string16" },
                    "compare": { "type": "string", "default": "exact" },
                    "writable_only": { "type": "boolean", "default": true },
                    "aligned": { "type": "boolean", "default": true },
                    "module": { "type": "string", "description": "Restrict to one module's address range" },
                    "limit": { "type": "integer", "default": 15, "description": "Candidates to return" }
                }),
                &[],
            ),
            access: Access::Session,
            example: r#"{"value":"37"}"#,
            group: "",
        },
        ToolDef {
            name: "scan_next",
            summary: "Narrow the current scan after the value changed in-game",
            description: "Filters the current candidates against fresh memory. \
                compare: exact (value) | changed | unchanged | increased | decreased | increased_by (value) | \
                decreased_by (value) | greater | less | between | not_equal. \
                Before calling, the user must have changed the value in-game (or deliberately not, for unchanged). \
                Usually 2-4 rounds reach a handful of candidates.",
            schema: obj(
                json!({
                    "compare": { "type": "string", "default": "exact" },
                    "value": { "type": "string" },
                    "value2": { "type": "string" },
                    "limit": { "type": "integer", "default": 15 }
                }),
                &[],
            ),
            access: Access::Session,
            example: r#"{"compare":"exact","value":"36"}"#,
            group: "",
        },
        ToolDef {
            name: "scan_results",
            summary: "Page through scan candidates with current and previous values",
            description: "Lists candidates in address order with live values. Use `type` to see one type of a multi-type scan.",
            schema: obj(
                json!({
                    "offset": { "type": "integer", "default": 0 },
                    "limit": { "type": "integer", "default": 50, "maximum": 1000 },
                    "type": { "type": "string" }
                }),
                &[],
            ),
            access: Access::Read,
            example: r#"{"limit":20}"#,
            group: "",
        },
        ToolDef {
            name: "scan_undo",
            summary: "Revert the last scan_next",
            description: "Restores the candidate set from before the last scan_next (one level).",
            schema: obj(json!({}), &[]),
            access: Access::Session,
            example: "{}",
            group: "",
        },
        ToolDef {
            name: "scan_reset",
            summary: "Drop the current scan",
            description: "Frees the scan session memory.",
            schema: obj(json!({}), &[]),
            access: Access::Session,
            example: "{}",
            group: "",
        },
        ToolDef {
            name: "cancel",
            summary: "Cancel a running scan or pointer scan",
            description: "Asks a long-running scan to stop early.",
            schema: obj(json!({}), &[]),
            access: Access::Session,
            example: "{}",
            group: "",
        },
    ]
}
