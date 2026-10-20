//! Understanding addresses, structures and code.

use super::{addr, dtype, obj, Access, ToolDef};
use serde_json::json;

pub(super) fn defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "analyze",
            summary: "Explain an address: region, module+offset, all type interpretations, pointer target, text",
            description: "One call to understand what an address is: which mapping it is in (heap/stack/image), \
                module+offset, the bytes interpreted as every numeric type, whether it looks like a pointer \
                (and to where) or a string. Use it to sanity-check scan candidates before writing.",
            schema: obj(json!({ "address": addr() }), &["address"]),
            access: Access::Read,
            example: r#"{"address":"0x7f12a0"}"#,
            group: "",
        },
        ToolDef {
            name: "dissect",
            summary: "Guess the structure around an address (pointers, floats, ints, strings)",
            description: "Heuristically types each field in `size` bytes starting at `address`: pointers (with targets), \
                floats, ints, zero runs and text. Start a bit before a found value (address-0x40) to see its neighbours: \
                related stats (max health, ammo in clip/reserve, position x/y/z) usually sit in the same object.",
            schema: obj(json!({ "address": addr(), "size": { "type": "integer", "default": 128, "maximum": 4096 } }), &["address"]),
            access: Access::Read,
            example: r#"{"address":"0x7f12a0-0x40","size":192}"#,
            group: "",
        },
        ToolDef {
            name: "pointer_scan",
            summary: "Find stable pointer paths (module+offset chains) to an address",
            description: "Searches chains like [[Game.exe+0x1A2B30]+0x18]+0x40 that end at `address`, so the value can be \
                found again after the game restarts. Takes seconds to a minute on big games. Best results: pass the \
                address of a value you already verified. Store the winning `expr` with table_set address=<expr>.",
            schema: obj(
                json!({
                    "address": addr(),
                    "max_depth": { "type": "integer", "default": 3, "maximum": 5 },
                    "max_offset": { "type": "string", "default": "0x1000", "description": "Max offset per level (hex)" },
                    "limit": { "type": "integer", "default": 10 }
                }),
                &["address"],
            ),
            access: Access::Read,
            example: r#"{"address":"0x7f12a0"}"#,
            group: "",
        },
        ToolDef {
            name: "disassemble",
            summary: "Disassemble x86/x64 code (Intel syntax)",
            description: "Decodes `count` instructions at `address`, with module+offset locations.",
            schema: obj(json!({ "address": addr(), "count": { "type": "integer", "default": 16, "maximum": 200 } }), &["address"]),
            access: Access::Read,
            example: r#"{"address":"Game.exe+0x4f2a0","count":12}"#,
            group: "",
        },
        ToolDef {
            name: "watch",
            summary: "Find the code that writes/accesses an address (hardware watchpoint)",
            description: "Arms a CPU debug-register watchpoint on every thread for `duration_ms`, while the user triggers \
                the change in-game (tell them to before calling!). Returns each instruction that wrote/accessed the \
                address with hit count and the registers at the first hit. Useful to find the struct base register \
                (e.g. rcx+0x18) or code to patch. Briefly traces the game; needs user approval for agents.",
            schema: obj(
                json!({
                    "address": addr(),
                    "size": { "type": "integer", "default": 4, "enum": [1, 2, 4, 8] },
                    "kind": { "type": "string", "default": "write", "enum": ["write", "access"] },
                    "duration_ms": { "type": "integer", "default": 5000, "maximum": 60000 },
                    "max_hits": { "type": "integer", "default": 2000 }
                }),
                &["address"],
            ),
            access: Access::Mutate,
            example: r#"{"address":"0x7f12a0","kind":"write","duration_ms":6000}"#,
            group: "",
        },
        ToolDef {
            name: "sample",
            summary: "Record a value over time (observe how it changes)",
            description: "Reads `address` `samples` times every `interval_ms` and returns the series plus min/max/changes. \
                Good for telling a timer from a counter, or checking a candidate reacts to gameplay.",
            schema: obj(
                json!({
                    "address": addr(), "type": dtype("i32"),
                    "samples": { "type": "integer", "default": 10, "maximum": 200 },
                    "interval_ms": { "type": "integer", "default": 250 }
                }),
                &["address"],
            ),
            access: Access::Read,
            example: r#"{"address":"0x7f12a0","type":"f32","samples":20,"interval_ms":100}"#,
            group: "",
        },
    ]
}
