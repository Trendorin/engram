//! The Settings tab: shows `engram_proto::config::Config`, validates and saves
//! it, applies what can change while running and tells the engine to reload.

use crate::ctx::Ctx;
use crate::format::tilde;
use crate::launcher;
use crate::view;
use crate::{poll, App, MainWindow};
use engram_proto::config::Config;
use engram_proto::paths;
use serde_json::json;
use slint::{ComponentHandle, Model};

const POLICIES: [&str; 3] = ["ask", "allow", "deny"];

/// Fill the tab (and the paths/version) from `c`.
pub fn show(w: &MainWindow, c: &Config) {
    let app = w.global::<App>();
    let agents: Vec<String> = app.get_agents().iter().map(|a| a.to_string()).collect();
    app.set_agent_index(agents.iter().position(|a| *a == c.agent).unwrap_or(0) as i32);
    app.set_set_terminal(c.terminal.clone().into());
    app.set_set_workspace(c.workspace.clone().into());
    app.set_set_policy_index(POLICIES.iter().position(|p| *p == c.policy).unwrap_or(0) as i32);
    app.set_set_timeout(c.approval_timeout_s as i32);
    app.set_set_rows(c.result_rows as i32);
    app.set_set_refresh(c.refresh_ms as i32);
    app.set_set_freeze(c.freeze_interval_ms as i32);
}

/// One-time setup at startup.
pub fn init(w: &MainWindow, c: &Config) {
    let app = w.global::<App>();
    app.set_version(engram_proto::VERSION.into());
    app.set_config_path(tilde(&paths::config_file()).into());
    app.set_data_path(tilde(&paths::data_dir()).into());
    app.set_logs_path(tilde(&paths::state_dir()).into());
    app.set_terminal_detected(launcher::detect_terminal().unwrap_or_default().into());
    app.set_workspace_default(tilde(&paths::default_agent_workspace()).into());
    app.set_workspace(tilde(&paths::agent_workspace()).into());
    show(w, c);
}

fn note(w: &MainWindow, text: &str, error: bool) {
    let app = w.global::<App>();
    app.set_settings_note(text.into());
    app.set_settings_error(error);
}

/// Validate the fields, save, and apply.
pub fn save(w: &MainWindow, ctx: &Ctx) {
    let app = w.global::<App>();
    let mut c = Config::load();
    let agent = app.get_agents().row_data(app.get_agent_index().max(0) as usize).map(|a| a.to_string()).unwrap_or_default();
    let fields = [
        ("agent", agent),
        ("terminal", app.get_set_terminal().to_string()),
        ("workspace", app.get_set_workspace().to_string()),
        ("policy", POLICIES.get(app.get_set_policy_index().max(0) as usize).unwrap_or(&"ask").to_string()),
        ("approval_timeout_s", app.get_set_timeout().to_string()),
        ("result_rows", app.get_set_rows().to_string()),
        ("refresh_ms", app.get_set_refresh().to_string()),
        ("freeze_interval_ms", app.get_set_freeze().to_string()),
    ];
    for (key, value) in fields {
        if let Err(e) = c.set(key, &value) {
            return note(w, &e, true);
        }
    }
    if let Err(e) = c.save() {
        return note(w, &format!("Cannot write {}: {e}", tilde(&paths::config_file())), true);
    }
    // Live changes: the window's refresh and result count, the engine's timeouts.
    view::with(|st| st.result_rows = c.result_rows);
    poll::set_interval(c.refresh_ms);
    app.set_workspace(tilde(&paths::agent_workspace()).into());
    ctx.quick("reload_config", json!({}), |_, _| {});
    note(w, "Saved. The start-up policy applies the next time the engine starts.", false);
}

/// Put the defaults into the fields; nothing is written until Save.
pub fn reset(w: &MainWindow) {
    show(w, &Config::default());
    note(w, "Defaults restored in the form — press Save to keep them.", false);
}
