//! Shared handles (window + workers) and the engine calls every view uses:
//! refreshes, attach, scan completion. Event handling is in `events.rs`, the
//! Inspect tab in `inspect.rs`.

use crate::engine::Worker;
use crate::format::{self, s};
use crate::view::{self, Level};
use crate::{App, MainWindow};
use engram_proto::RpcError;
use serde_json::{json, Value};
use slint::ComponentHandle;

/// Handles shared by every callback: the window and the two workers.
#[derive(Clone)]
pub(crate) struct Ctx {
    pub(crate) ui: slint::Weak<MainWindow>,
    /// Short calls (reads, table edits, approvals, cancel).
    pub(crate) quick: Worker,
    /// Long calls (scans, pointer scan, watch, sample, launch).
    pub(crate) ops: Worker,
}

pub(crate) type Done = Box<dyn FnOnce(&MainWindow, Result<Value, RpcError>) + Send>;

impl Ctx {
    pub(crate) fn call_on(&self, worker: &Worker, method: &'static str, params: Value, done: Done) {
        let ui = self.ui.clone();
        worker.run(move |c| {
            let r = c.call(method, params);
            let _ = ui.upgrade_in_event_loop(move |w| done(&w, r));
        });
    }

    pub(crate) fn quick(
        &self,
        method: &'static str,
        params: Value,
        done: impl FnOnce(&MainWindow, Result<Value, RpcError>) + Send + 'static,
    ) {
        self.call_on(&self.quick, method, params, Box::new(done));
    }

    pub(crate) fn ops(
        &self,
        method: &'static str,
        params: Value,
        done: impl FnOnce(&MainWindow, Result<Value, RpcError>) + Send + 'static,
    ) {
        self.call_on(&self.ops, method, params, Box::new(done));
    }

    /// Run blocking non-engine work (terminal, setup) off the UI thread.
    pub(crate) fn background(&self, f: impl FnOnce() -> Result<String, String> + Send + 'static) {
        let ui = self.ui.clone();
        std::thread::spawn(move || {
            let r = f();
            let _ = ui.upgrade_in_event_loop(move |w| match r {
                Ok(m) => view::message(&w, Level::Success, m),
                Err(m) => view::message(&w, Level::Error, m),
            });
        });
    }

    // ---- refreshes ------------------------------------------------------------

    pub(crate) fn refresh_status(&self) {
        let me = self.clone();
        self.quick("status", json!({}), move |w, r| match r {
            Ok(v) => {
                if view::apply_status(w, &v) {
                    me.refresh_results(w);
                }
            }
            Err(e) => view::error(w, "Status", &e),
        });
    }

    pub(crate) fn refresh_results(&self, w: &MainWindow) {
        let app = w.global::<App>();
        let (filter, rows) = view::with(|st| (st.type_filter(app.get_scan_type_filter().max(0) as usize), st.result_rows));
        let mut params = json!({ "offset": 0, "limit": rows });
        if let Some(t) = filter {
            params["type"] = json!(t);
        }
        self.quick("scan_results", params, |w, r| {
            if let Ok(v) = r {
                view::apply_results(w, &v);
            }
        });
    }

    pub(crate) fn refresh_table(&self) {
        self.quick("table_list", json!({}), |w, r| {
            if let Ok(v) = r {
                view::apply_table(w, &v);
            }
        });
    }

    pub(crate) fn resync(&self) {
        self.refresh_status();
        self.refresh_table();
        self.quick("activity", json!({ "limit": 300 }), |_, r| {
            if let Ok(v) = r {
                view::apply_activity(&v);
            }
        });
    }

    pub(crate) fn refresh_processes(&self, include_helpers: bool) {
        if let Some(w) = self.ui.upgrade() {
            w.global::<App>().set_picker_loading(true);
        }
        self.quick("processes", json!({ "include_helpers": include_helpers, "limit": 1000 }), |w, r| {
            let app = w.global::<App>();
            app.set_picker_loading(false);
            match r {
                Ok(v) => {
                    app.set_picker_error("".into());
                    view::apply_processes(w, &v);
                }
                Err(e) => app.set_picker_error(format::error(&e).into()),
            }
        });
    }

    // ---- events -----------------------------------------------------------------

    pub(crate) fn attach(&self, pid: i32) {
        let me = self.clone();
        self.quick("attach", json!({ "pid": pid }), move |w, r| {
            let app = w.global::<App>();
            match r {
                Ok(v) => {
                    app.set_picker_open(false);
                    app.set_picker_error("".into());
                    let loaded = v["table_entries_loaded"].as_u64().unwrap_or(0);
                    let name = s(&v["attached"]["name"]).to_string();
                    let msg = match loaded {
                        0 => crate::i18n::tf("Attached to {}", &[&name]),
                        n => crate::i18n::tf("Attached to {}; loaded {} saved address(es)", &[&name, &n]),
                    };
                    view::message(w, Level::Success, msg);
                    me.refresh_status();
                    me.refresh_table();
                }
                Err(e) => app.set_picker_error(format::error(&e).into()),
            }
        });
    }

    pub(crate) fn scan_done(&self, w: &MainWindow, r: Result<Value, RpcError>) {
        match r {
            Ok(v) => {
                let total = v["total"].as_u64().unwrap_or(0);
                let level = if total == 0 { Level::Warning } else { Level::Info };
                let mut msg = format!("{} result(s)", format::thousands(total));
                if let Some(mb) = v["scanned_mb"].as_u64().filter(|mb| *mb > 0) {
                    msg.push_str(&format!(" in {} MB", format::thousands(mb)));
                }
                if total == 0 {
                    msg.push_str(". No matches: try another type, or compare=unknown and narrow down.");
                }
                view::message(w, level, msg);
            }
            Err(e) => view::error(w, "Scan", &e),
        }
        w.global::<App>().set_scan_type_filter(0);
        self.refresh_status();
    }
}
