//! Periodic refresh of live values (address table and visible scan results).

use crate::ctx::Ctx;
use crate::view;
use crate::App;
use serde_json::json;
use slint::{ComponentHandle, Model, Timer, TimerMode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

thread_local! {
    static TIMER: Timer = Timer::default();
}

/// Keep table values and visible results live while nothing else is running.
pub(crate) fn start(ctx: &Ctx, interval_ms: u64) {
    let inflight = Arc::new(AtomicBool::new(false));
    let c = ctx.clone();
    let tick = move || {
        let Some(w) = c.ui.upgrade() else { return };
        let app = w.global::<App>();
        if !app.get_connected() || !app.get_attached() || inflight.swap(true, Ordering::AcqRel) {
            return;
        }
        let want_table = app.get_entries().row_count() > 0;
        let want_results = app.get_has_scan() && !app.get_busy() && app.get_results().row_count() > 0;
        let (filter, rows) = view::with(|st| (st.type_filter(app.get_scan_type_filter().max(0) as usize), st.result_rows));
        let ui = c.ui.clone();
        let flag = inflight.clone();
        c.quick.run(move |conn| {
            let table = want_table.then(|| conn.call("table_list", json!({})).ok()).flatten();
            let results = want_results
                .then(|| {
                    let mut p = json!({ "offset": 0, "limit": rows });
                    if let Some(t) = filter {
                        p["type"] = json!(t);
                    }
                    conn.call("scan_results", p).ok()
                })
                .flatten();
            let _ = ui.upgrade_in_event_loop(move |w| {
                if let Some(v) = table {
                    view::apply_table(&w, &v);
                }
                if let Some(v) = results {
                    view::apply_results(&w, &v);
                }
            });
            flag.store(false, Ordering::Release);
        });
    };
    TIMER.with(|t| t.start(TimerMode::Repeated, Duration::from_millis(interval_ms), tick));
}

/// Apply a new refresh interval from the settings.
pub(crate) fn set_interval(ms: u64) {
    TIMER.with(|t| t.set_interval(Duration::from_millis(ms)));
}
