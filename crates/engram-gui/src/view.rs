//! UI-thread state: the models behind the views and how engine replies and
//! events are applied to them. Everything here runs on the Slint event loop.

use crate::format::{self, s, text};
use crate::{ActivityRow, App, ApprovalRow, EntryRow, MainWindow};
use engram_proto::RpcError;
use serde_json::Value;
use slint::language::StandardListViewItem;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub type Row = ModelRc<StandardListViewItem>;

const MAX_ACTIVITY: usize = 300;

pub struct Process {
    pub pid: i32,
    name: String,
    kind: String,
    rss_mb: u64,
    cmdline: String,
}

pub struct State {
    pub results: Rc<VecModel<Row>>,
    pub entries: Rc<VecModel<EntryRow>>,
    pub activity: Rc<VecModel<ActivityRow>>,
    pub approvals: Rc<VecModel<ApprovalRow>>,
    pub processes: Rc<VecModel<Row>>,
    all_processes: Vec<Process>,
    /// pid of each shown (filtered) process row.
    pub shown_pids: Vec<i32>,
    filter: String,
    last_click: Option<(&'static str, i32, Instant)>,
    /// Type names offered by the result filter ("All types" first).
    type_filters: Vec<String>,
    /// Scan results requested from the engine (setting `result_rows`).
    pub result_rows: u64,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(s.borrow_mut().as_mut().expect("view::init ran")))
}

/// Create the models and hand them to the window.
pub fn init(w: &MainWindow, result_rows: u64) {
    let st = State {
        results: Rc::new(VecModel::default()),
        entries: Rc::new(VecModel::default()),
        activity: Rc::new(VecModel::default()),
        approvals: Rc::new(VecModel::default()),
        processes: Rc::new(VecModel::default()),
        all_processes: Vec::new(),
        shown_pids: Vec::new(),
        filter: String::new(),
        last_click: None,
        type_filters: vec!["All types".into()],
        result_rows,
    };
    let app = w.global::<App>();
    app.set_results(st.results.clone().into());
    app.set_entries(st.entries.clone().into());
    app.set_activity(st.activity.clone().into());
    app.set_approvals(st.approvals.clone().into());
    app.set_processes(st.processes.clone().into());
    STATE.with(|s| *s.borrow_mut() = Some(st));
}

fn row(cells: &[&str]) -> Row {
    let items: Vec<StandardListViewItem> = cells.iter().map(|c| StandardListViewItem::from(*c)).collect();
    ModelRc::new(VecModel::from(items))
}

fn row_text(r: &Row, col: usize) -> String {
    r.row_data(col).map(|i| i.text.to_string()).unwrap_or_default()
}

/// Replace `model` with `new`, updating rows in place when the keys match so
/// selection and scroll position survive periodic refreshes.
fn sync<T: Clone + 'static>(model: &VecModel<T>, new: Vec<T>, same_key: impl Fn(&T, &T) -> bool, same: impl Fn(&T, &T) -> bool) {
    let keys_match =
        model.row_count() == new.len() && new.iter().enumerate().all(|(i, n)| model.row_data(i).is_some_and(|o| same_key(&o, n)));
    if !keys_match {
        model.set_vec(new);
        return;
    }
    for (i, n) in new.into_iter().enumerate() {
        if model.row_data(i).is_some_and(|o| !same(&o, &n)) {
            model.set_row_data(i, n);
        }
    }
}

fn same_row(a: &Row, b: &Row) -> bool {
    a.row_count() == b.row_count() && (0..a.row_count()).all(|i| row_text(a, i) == row_text(b, i))
}

impl State {
    /// True if this click on (`table`, `row`) completes a double-click.
    pub fn double_click(&mut self, table: &'static str, row: i32) -> bool {
        let now = Instant::now();
        let hit = matches!(self.last_click, Some((t, r, at)) if t == table && r == row && now - at < Duration::from_millis(450));
        self.last_click = if hit { None } else { Some((table, row, now)) };
        hit
    }

    /// (address, type, value) of a result row.
    pub fn result(&self, row: usize) -> Option<(String, String, String)> {
        let r = self.results.row_data(row)?;
        Some((row_text(&r, 0), row_text(&r, 1), row_text(&r, 2)))
    }

    pub fn type_filter(&self, index: usize) -> Option<String> {
        (index > 0).then(|| self.type_filters.get(index).cloned()).flatten()
    }
}

// ---- messages -----------------------------------------------------------------

pub enum Level {
    Info = 0,
    Success = 1,
    Warning = 2,
    Error = 3,
}

pub fn message(w: &MainWindow, level: Level, text: impl Into<SharedString>) {
    let app = w.global::<App>();
    app.set_message(text.into());
    app.set_message_level(level as i32);
}

pub fn error(w: &MainWindow, prefix: &str, e: &RpcError) {
    message(w, Level::Error, format!("{prefix}: {}", format::error(e)));
}

// ---- status / scan --------------------------------------------------------------

/// Apply a `status` reply. Returns true if a scan exists (results need a refresh).
pub fn apply_status(w: &MainWindow, v: &Value) -> bool {
    let app = w.global::<App>();
    app.set_connected(true);
    app.set_access(s(&v["access"]).into());
    app.set_policy_index(match s(&v["policy"]) {
        "allow" => 1,
        "deny" => 2,
        _ => 0,
    });
    let t = &v["target"];
    let attached = t.is_object();
    app.set_attached(attached);
    if attached {
        let pid = t["pid"].as_i64().unwrap_or(0) as i32;
        app.set_target_name(s(&t["name"]).into());
        app.set_target_pid(pid);
        app.set_target_kind(s(&t["kind"]).into());
        app.set_paused(is_stopped(pid));
    } else {
        app.set_paused(false);
    }
    match v["busy"].as_str() {
        Some(op) => {
            app.set_busy(true);
            app.set_busy_op(op.into());
        }
        None => app.set_busy(false),
    }
    let scan = &v["scan"];
    if !scan.is_object() {
        app.set_has_scan(false);
        app.set_scan_total(0);
        app.set_scan_step(0);
        app.set_can_undo(false);
        app.set_scan_types_text("".into());
        set_type_filters(w, vec![]);
        with(|st| st.results.set_vec(vec![]));
        app.set_results_note("".into());
        return false;
    }
    app.set_has_scan(true);
    app.set_scan_total(scan["total"].as_i64().unwrap_or(0).min(i32::MAX as i64) as i32);
    app.set_scan_step(scan["step"].as_i64().unwrap_or(0) as i32);
    app.set_can_undo(scan["can_undo"] == Value::Bool(true));
    let by_type: Vec<(String, u64)> =
        scan["by_type"].as_object().map(|m| m.iter().map(|(k, c)| (k.clone(), c.as_u64().unwrap_or(0))).collect()).unwrap_or_default();
    let types_text = if by_type.len() > 1 {
        by_type.iter().filter(|(_, c)| *c > 0).map(|(k, c)| format!("{k} {}", format::thousands(*c))).collect::<Vec<_>>().join(" · ")
    } else {
        by_type.first().map(|(k, _)| k.clone()).unwrap_or_default()
    };
    app.set_scan_types_text(types_text.into());
    set_type_filters(w, by_type.iter().filter(|(_, c)| *c > 0).map(|(k, _)| k.clone()).collect());
    true
}

fn set_type_filters(w: &MainWindow, types: Vec<String>) {
    let mut list = vec!["All types".to_string()];
    list.extend(types);
    let changed = with(|st| {
        if st.type_filters == list {
            return false;
        }
        st.type_filters = list.clone();
        true
    });
    if changed {
        let app = w.global::<App>();
        let model: Vec<SharedString> = list.iter().map(|t| t.into()).collect();
        app.set_scan_type_filters(ModelRc::new(VecModel::from(model)));
        app.set_scan_type_filter(0);
    }
}

/// Process state 'T' (stopped) in /proc/<pid>/stat.
fn is_stopped(pid: i32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|st| st.rfind(')').and_then(|i| st[i + 1..].trim_start().chars().next()))
        .is_some_and(|c| c == 'T')
}

pub fn apply_results(w: &MainWindow, v: &Value) {
    let list = v["results"].as_array().cloned().unwrap_or_default();
    let rows: Vec<Row> = list
        .iter()
        .map(|r| {
            let value = r["value"].as_str().unwrap_or("??");
            let prev = r["previous"].as_str().unwrap_or("");
            row(&[s(&r["address"]), s(&r["type"]), value, prev, s(&r["where"])])
        })
        .collect();
    let total = v["total"].as_u64().unwrap_or(0);
    let shown = rows.len() as u64;
    with(|st| sync(&st.results, rows, |a, b| row_text(a, 0) == row_text(b, 0) && row_text(a, 1) == row_text(b, 1), same_row));
    let note = if total > shown {
        format!("Showing the first {} of {}. Narrow down with Next scan.", format::thousands(shown), format::thousands(total))
    } else if total > 0 {
        "Double-click a result to add it to the address table.".to_string()
    } else {
        String::new()
    };
    w.global::<App>().set_results_note(note.into());
}

// ---- address table ----------------------------------------------------------------

pub fn apply_table(w: &MainWindow, v: &Value) {
    let entries: Vec<EntryRow> = v["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| EntryRow {
            id: e["id"].as_i64().unwrap_or(0) as i32,
            description: s(&e["description"]).into(),
            address: s(&e["address"]).into(),
            resolved: text(&e["resolved"]).into(),
            r#type: text(&e["type"]).into(),
            value: text(&e["value"]).into(),
            frozen: e["frozen"] == Value::Bool(true),
        })
        .collect();
    with(|st| sync(&st.entries, entries, |a, b| a.id == b.id, |a, b| a == b));
    w.global::<App>().set_table_game(text(&v["game"]).into());
}

// ---- activity / approvals ------------------------------------------------------------

fn activity_row(a: &Value) -> ActivityRow {
    let tool = s(&a["tool"]);
    ActivityRow {
        time: format::clock(a["ts"].as_u64().unwrap_or(0)).into(),
        client: s(&a["client"]).into(),
        tool: tool.into(),
        summary: s(&a["summary"]).into(),
        ok: a["ok"] != Value::Bool(false),
        notify: tool == "notify",
    }
}

pub fn apply_activity(v: &Value) {
    let mut rows: Vec<ActivityRow> = v["activity"].as_array().into_iter().flatten().map(activity_row).collect();
    rows.reverse();
    with(|st| st.activity.set_vec(rows));
}

pub fn push_activity(a: &Value) {
    with(|st| {
        st.activity.insert(0, activity_row(a));
        while st.activity.row_count() > MAX_ACTIVITY {
            st.activity.remove(st.activity.row_count() - 1);
        }
    });
}

pub fn push_approval(v: &Value) {
    let r = ApprovalRow {
        id: v["id"].as_i64().unwrap_or(0) as i32,
        client: s(&v["client"]).into(),
        tool: s(&v["tool"]).into(),
        summary: s(&v["summary"]).into(),
    };
    with(|st| st.approvals.push(r));
}

pub fn remove_approval(id: i32) {
    with(|st| {
        if let Some(i) = st.approvals.iter().position(|a| a.id == id) {
            st.approvals.remove(i);
        }
    });
}

// ---- processes ---------------------------------------------------------------------------

pub fn apply_processes(w: &MainWindow, v: &Value) {
    let list: Vec<Process> = v["processes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| Process {
            pid: p["pid"].as_i64().unwrap_or(0) as i32,
            name: s(&p["name"]).into(),
            kind: s(&p["kind"]).into(),
            rss_mb: p["rss_mb"].as_u64().unwrap_or(0),
            cmdline: s(&p["cmdline"]).into(),
        })
        .collect();
    let hidden = v["hidden_helpers"].as_u64().unwrap_or(0);
    let note = match hidden {
        0 => format!("{} processes", list.len()),
        1 => format!("{} processes · 1 helper hidden", list.len()),
        h => format!("{} processes · {h} helpers hidden", list.len()),
    };
    w.global::<App>().set_processes_note(note.into());
    with(|st| {
        st.all_processes = list;
        st.refilter();
    });
}

pub fn filter_processes(text: &str) {
    with(|st| {
        st.filter = text.trim().to_lowercase();
        st.refilter();
    });
}

impl State {
    fn refilter(&mut self) {
        let f = &self.filter;
        let shown: Vec<&Process> = self
            .all_processes
            .iter()
            .filter(|p| {
                f.is_empty() || p.name.to_lowercase().contains(f) || p.cmdline.to_lowercase().contains(f) || p.pid.to_string() == *f
            })
            .collect();
        self.shown_pids = shown.iter().map(|p| p.pid).collect();
        let rows: Vec<Row> = shown
            .iter()
            .map(|p| row(&[&p.name, &p.pid.to_string(), &p.kind, &format!("{} MB", format::thousands(p.rss_mb)), &p.cmdline]))
            .collect();
        self.processes.set_vec(rows);
    }
}
