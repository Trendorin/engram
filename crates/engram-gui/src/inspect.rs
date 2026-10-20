//! The Inspect tab: one engine call per action, rendered as text.

use crate::ctx::Ctx;
use crate::format;
use crate::view;
use crate::{App, MainWindow};
use engram_proto::RpcError;
use serde_json::{json, Value};
use slint::ComponentHandle;

impl Ctx {
    pub(crate) fn inspect(&self, action: &str, address: &str, ty: &str) {
        let Some(w) = self.ui.upgrade() else { return };
        let app = w.global::<App>();
        let address = address.trim().to_string();
        if address.is_empty() {
            return;
        }
        let size = match ty {
            "i8" | "u8" => 1,
            "i16" | "u16" => 2,
            "i64" | "u64" | "f64" | "ptr" => 8,
            _ => 4,
        };
        let (method, params, title, long): (&'static str, Value, String, bool) = match action {
            "analyze" => ("analyze", json!({ "address": address }), format!("Analyze {address}"), false),
            "dissect" => ("dissect", json!({ "address": address, "size": 256 }), format!("Structure at {address}"), false),
            "disassemble" => ("disassemble", json!({ "address": address, "count": 40 }), format!("Code at {address}"), false),
            "hexdump" => ("hexdump", json!({ "address": address, "size": 512 }), format!("Memory at {address}"), false),
            "sample" => (
                "sample",
                json!({ "address": address, "type": ty, "samples": 20, "interval_ms": 150 }),
                format!("Sampling {address} as {ty} for 3 s…"),
                true,
            ),
            "pointer_scan" => ("pointer_scan", json!({ "address": address, "limit": 25 }), format!("Pointer paths to {address}…"), true),
            "watch_write" | "watch_access" => {
                let kind = if action == "watch_write" { "write" } else { "access" };
                (
                    "watch",
                    json!({ "address": address, "size": size, "kind": kind, "duration_ms": 8000 }),
                    format!("Watching {kind}s to {address} for 8 s — trigger the change in-game now"),
                    true,
                )
            }
            _ => return,
        };
        app.set_inspect_running(true);
        app.set_inspect_title(title.into());
        app.set_side_tab(1);
        let action = action.to_string();
        let done = move |w: &MainWindow, r: Result<Value, RpcError>| {
            let app = w.global::<App>();
            app.set_inspect_running(false);
            match r {
                Ok(v) => {
                    let out = match action.as_str() {
                        "analyze" => format::analyze(&v),
                        "dissect" => format::dissect(&v),
                        "disassemble" => format::disassemble(&v),
                        "hexdump" => format::hexdump(&v),
                        "sample" => format::sample(&v),
                        "pointer_scan" => format::pointer_scan(&v),
                        _ => format::watch(&v),
                    };
                    let title = app.get_inspect_title().trim_end_matches('…').to_string();
                    app.set_inspect_title(title.split(" for 8 s").next().unwrap_or(&title).to_string().into());
                    app.set_inspect_output(out.into());
                }
                Err(e) => {
                    app.set_inspect_output(format::error(&e).into());
                    view::error(w, "Inspect", &e);
                }
            }
        };
        if long {
            self.ops(method, params, done);
        } else {
            self.quick(method, params, done);
        }
    }
}
