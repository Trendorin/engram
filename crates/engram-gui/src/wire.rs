//! Connects the window's callbacks (`App` global) to engine calls.

use crate::ctx::Ctx;
use crate::format::{self, s};
use crate::launcher;
use crate::view::{self, Level};
use crate::{App, MainWindow};
use serde_json::json;
use slint::{ComponentHandle, Timer};
use std::time::Duration;

/// Wire every callback; one function per area of the window.
pub(crate) fn wire(w: &MainWindow, ctx: &Ctx) {
    let app = w.global::<App>();
    picker(&app, ctx);
    scan(&app, ctx);
    table(&app, ctx);
    agents(&app, ctx);
    inspect(&app, ctx);
    settings(&app, ctx);
}

/// Process picker: list, filter, attach, start a program; target controls.
fn picker(app: &App, ctx: &Ctx) {
    let c = ctx.clone();
    app.on_refresh_processes(move |helpers| c.refresh_processes(helpers));
    app.on_filter_processes(|text| view::filter_processes(&text));
    let c = ctx.clone();
    app.on_process_clicked(move |row| {
        if view::with(|st| st.double_click("processes", row)) {
            if let Some(pid) = view::with(|st| st.shown_pids.get(row as usize).copied()) {
                c.attach(pid);
            }
        }
    });
    let c = ctx.clone();
    app.on_attach_row(move |row| {
        if let Some(pid) = view::with(|st| st.shown_pids.get(row as usize).copied()) {
            c.attach(pid);
        }
    });
    let c = ctx.clone();
    app.on_launch(move |line| {
        let argv = format::split_command(&line);
        if argv.is_empty() {
            return;
        }
        let cwd = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        let me = c.clone();
        c.ops("launch", json!({ "argv": argv, "cwd": cwd }), move |w, r| match r {
            Ok(v) => {
                view::message(w, Level::Success, format!("Started pid {}. Pick it (or its game process) in the list.", v["pid"]));
                let helpers = false;
                let me2 = me.clone();
                Timer::single_shot(Duration::from_millis(1200), move || me2.refresh_processes(helpers));
            }
            Err(e) => w.global::<App>().set_picker_error(format::error(&e).into()),
        });
    });
    let c = ctx.clone();
    app.on_detach(move || {
        c.quick("detach", json!({}), |w, r| match r {
            Ok(_) => view::message(w, Level::Info, "Detached"),
            Err(e) => view::error(w, "Detach", &e),
        })
    });
    let c = ctx.clone();
    app.on_toggle_pause(move || {
        let Some(w) = c.ui.upgrade() else { return };
        let action = if w.global::<App>().get_paused() { "resume" } else { "pause" };
        c.quick("process_control", json!({ "action": action }), |w, r| match r {
            Ok(v) => w.global::<App>().set_paused(v["paused"] == json!(true)),
            Err(e) => view::error(w, "Pause", &e),
        });
    });
    let c = ctx.clone();
    app.on_grant_access(move || {
        if let Some(w) = c.ui.upgrade() {
            view::message(&w, Level::Info, "Running engram setup — confirm the password prompt…");
        }
        c.background(launcher::grant_access);
    });
}

/// Value scan controls and the result list.
fn scan(app: &App, ctx: &Ctx) {
    let c = ctx.clone();
    app.on_first_scan(move |value, value2, ty, compare, writable, aligned| {
        let mut p = json!({ "type": ty.as_str(), "compare": compare.as_str(), "writable_only": writable, "aligned": aligned, "limit": 0 });
        if !value.is_empty() {
            p["value"] = json!(value.as_str());
        }
        if !value2.is_empty() {
            p["value2"] = json!(value2.as_str());
        }
        if let Some(w) = c.ui.upgrade() {
            let app = w.global::<App>();
            app.set_busy(true);
            app.set_busy_op("scan".into());
            app.set_progress(0.0);
        }
        let me = c.clone();
        c.ops("scan", p, move |w, r| me.scan_done(w, r));
    });
    let c = ctx.clone();
    app.on_next_scan(move |compare, value, value2| {
        let mut p = json!({ "compare": compare.as_str(), "limit": 0 });
        if !value.is_empty() {
            p["value"] = json!(value.as_str());
        }
        if !value2.is_empty() {
            p["value2"] = json!(value2.as_str());
        }
        if let Some(w) = c.ui.upgrade() {
            let app = w.global::<App>();
            app.set_busy(true);
            app.set_busy_op("scan".into());
            app.set_progress(0.0);
        }
        let me = c.clone();
        c.ops("scan_next", p, move |w, r| me.scan_done(w, r));
    });
    let c = ctx.clone();
    app.on_undo_scan(move || {
        let me = c.clone();
        c.quick("scan_undo", json!({}), move |w, r| match r {
            Ok(v) => {
                view::message(w, Level::Info, format!("Restored {} result(s)", format::thousands(v["total"].as_u64().unwrap_or(0))));
                me.refresh_status();
            }
            Err(e) => view::error(w, "Undo", &e),
        });
    });
    let c = ctx.clone();
    app.on_reset_scan(move || {
        let me = c.clone();
        c.quick("scan_reset", json!({}), move |w, r| {
            if let Err(e) = r {
                view::error(w, "Reset", &e);
            }
            me.refresh_status();
        });
    });
    let c = ctx.clone();
    app.on_cancel(move || c.quick("cancel", json!({}), |_, _| {}));
    let c = ctx.clone();
    app.on_filter_results(move |_| {
        if let Some(w) = c.ui.upgrade() {
            c.refresh_results(&w);
        }
    });
    let c = ctx.clone();
    app.on_result_clicked(move |row| {
        let Some(w) = c.ui.upgrade() else { return };
        if let Some((addr, _, _)) = view::with(|st| st.result(row as usize)) {
            w.global::<App>().set_inspect_address(addr.into());
        }
        if view::with(|st| st.double_click("results", row)) {
            w.global::<App>().invoke_add_result(row);
        }
    });
    let c = ctx.clone();
    app.on_add_result(move |row| {
        let Some((addr, ty, value)) = view::with(|st| st.result(row as usize)) else { return };
        let ty = format::entry_type(&ty, &value);
        c.quick("table_add", json!({ "address": addr, "type": ty }), move |w, r| match r {
            Ok(_) => view::message(w, Level::Success, format!("Added {addr} ({ty}) to the address table")),
            Err(e) => view::error(w, "Add", &e),
        });
    });
}

/// Address table edits.
fn table(app: &App, ctx: &Ctx) {
    let c = ctx.clone();
    app.on_add_entry(move |address, ty, description| {
        let mut p = json!({ "address": address.trim(), "type": if ty.is_empty() { "i32" } else { ty.as_str() } });
        if !description.trim().is_empty() {
            p["description"] = json!(description.trim());
        }
        c.quick("table_add", p, |w, r| match r {
            Ok(_) => view::message(w, Level::Success, "Entry added"),
            Err(e) => view::error(w, "Add", &e),
        });
    });
    let c = ctx.clone();
    app.on_update_entry(move |id, description, address, ty, value| {
        let mut p = json!({ "id": id, "description": description.as_str(), "address": address.trim() });
        if !ty.trim().is_empty() {
            p["type"] = json!(ty.trim());
        }
        let writes = !value.is_empty();
        if writes {
            p["value"] = json!(value.as_str());
        }
        let me = c.clone();
        c.quick("table_set", p, move |w, r| match r {
            Ok(v) if writes => view::message(w, Level::Success, format!("Wrote {}", format::text(&v["value"]))),
            Ok(_) => view::message(w, Level::Info, "Entry updated"),
            Err(e) => {
                view::error(w, "Update", &e);
                me.refresh_table();
            }
        });
    });
    let c = ctx.clone();
    app.on_freeze_entry(move |id, frozen| {
        c.quick("table_set", json!({ "id": id, "freeze": frozen }), |w, r| {
            if let Err(e) = r {
                view::error(w, "Freeze", &e);
            }
        });
    });
    let c = ctx.clone();
    app.on_remove_entry(move |id| {
        c.quick("table_remove", json!({ "id": id }), |w, r| {
            if let Err(e) = r {
                view::error(w, "Remove", &e);
            }
        });
    });
}

/// Agent tab: policy, approvals, terminal.
fn agents(app: &App, ctx: &Ctx) {
    let c = ctx.clone();
    app.on_set_policy(move |index| {
        let mode = ["ask", "allow", "deny"].get(index as usize).copied().unwrap_or("ask");
        c.quick("policy", json!({ "mode": mode }), |w, r| match r {
            Ok(v) => view::message(w, Level::Info, format!("Agent writes: {}", s(&v["policy"]))),
            Err(e) => view::error(w, "Policy", &e),
        });
    });
    let c = ctx.clone();
    app.on_respond(move |id, decision| {
        view::remove_approval(id);
        c.quick("approval_respond", json!({ "id": id, "decision": decision.as_str() }), |w, r| {
            if let Err(e) = r {
                view::error(w, "Approval", &e);
            }
        });
    });
    let c = ctx.clone();
    app.on_open_terminal(move |agent| {
        let agent = agent.to_string();
        c.background(move || launcher::open_agent_terminal(&agent));
    });
}

/// Inspect tab.
fn inspect(app: &App, ctx: &Ctx) {
    let c = ctx.clone();
    app.on_inspect(move |action, address, ty| c.inspect(&action, &address, &ty));
}

/// Settings tab.
fn settings(app: &App, ctx: &Ctx) {
    let c = ctx.clone();
    app.on_save_settings(move || {
        if let Some(w) = c.ui.upgrade() {
            crate::settings::save(&w, &c);
        }
    });
    let c = ctx.clone();
    app.on_reset_settings(move || {
        if let Some(w) = c.ui.upgrade() {
            crate::settings::reset(&w);
        }
    });
    let c = ctx.clone();
    app.on_open_folder(move |path| {
        let path = path.to_string();
        c.background(move || launcher::open_folder(&path));
    });
}
