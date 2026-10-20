//! Engine events and connection changes, applied on the UI thread.

use crate::ctx::Ctx;
use crate::format::s;
use crate::view::{self, Level};
use crate::{App, MainWindow};
use engram_proto::{Event, RpcError};
use serde_json::json;
use slint::ComponentHandle;
use std::sync::atomic::{AtomicBool, Ordering};

static CONNECTED_BEFORE: AtomicBool = AtomicBool::new(false);

impl Ctx {
    pub(crate) fn on_event(&self, w: &MainWindow, e: Event) {
        let app = w.global::<App>();
        let d = &e.data;
        match e.event.as_str() {
            "activity" => view::push_activity(d),
            "notify" => {
                let level = match s(&d["level"]) {
                    "success" => Level::Success,
                    "warning" => Level::Warning,
                    _ => Level::Info,
                };
                view::message(w, level, format!("{}: {}", s(&d["client"]), s(&d["message"])));
            }
            "target" => {
                if let Some(p) = d["paused"].as_bool() {
                    app.set_paused(p);
                } else {
                    if d["attached"] == json!(false) && s(&d["reason"]) == "exited" {
                        view::message(w, Level::Warning, crate::i18n::tf("{} (pid {}) exited", &[&s(&d["name"]), &d["pid"]]));
                    }
                    self.refresh_status();
                    self.refresh_table();
                }
            }
            "scan" => self.refresh_status(),
            "table" | "memory" => self.refresh_table(),
            "progress" => {
                let running = d["running"] == json!(true);
                app.set_busy(running);
                app.set_busy_op(s(&d["op"]).into());
                app.set_progress(d["fraction"].as_f64().unwrap_or(0.0) as f32);
            }
            "approval_request" => {
                view::push_approval(d);
                view::message(w, Level::Warning, crate::i18n::tf("{} is waiting for your approval", &[&s(&d["client"])]));
            }
            "approval_resolved" => {
                view::remove_approval(d["id"].as_i64().unwrap_or(0) as i32);
                let (level, text) = match s(&d["decision"]) {
                    "allow" => (Level::Info, "Request allowed"),
                    "allow_session" => (Level::Info, "Request allowed; this agent may write without asking until the engine restarts"),
                    "deny" => (Level::Info, "Request denied"),
                    _ => (Level::Warning, "An approval request timed out"),
                };
                view::message(w, level, crate::i18n::t(text));
            }
            "policy" => app.set_policy_index(match s(&d["policy"]) {
                "allow" => 1,
                "deny" => 2,
                _ => 0,
            }),
            _ => {}
        }
    }

    pub(crate) fn on_connection(&self, w: &MainWindow, r: Result<(), RpcError>) {
        let app = w.global::<App>();
        match r {
            Ok(()) => {
                let again = CONNECTED_BEFORE.swap(true, Ordering::Relaxed);
                app.set_connected(true);
                // Requests from before a reconnect can no longer be answered.
                view::with(|st| st.approvals.set_vec(vec![]));
                view::message(w, Level::Info, crate::i18n::t(if again { "Reconnected to the engine" } else { "Engine ready" }));
                self.resync();
            }
            Err(e) => {
                app.set_connected(false);
                app.set_attached(false);
                app.set_busy(false);
                view::with(|st| st.approvals.set_vec(vec![]));
                view::error(w, "Engine", &e);
            }
        }
    }

    // ---- actions ------------------------------------------------------------------
}
