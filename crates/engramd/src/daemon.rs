//! Session state, events and the write-approval gate.

use crate::table::Table;
pub use crate::target::Target;
use engram_core::process;
use engram_core::scan::{Progress, ScanSession};
use engram_proto::config::Config;
use engram_proto::RpcError;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const GUI: &str = "gui";

pub fn core_err(e: engram_core::Error) -> RpcError {
    let mut r = RpcError::new(e.code, e.message);
    if e.code == "permission_denied" {
        r = r.hint("Engram lacks ptrace rights for this process. Run `engram setup` once (grants CAP_SYS_PTRACE to engramd), or start the game via `engram launch`.");
    }
    r
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Ask,
    Allow,
    Deny,
}

impl Policy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ask" => Some(Self::Ask),
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(Default)]
pub struct ScanState {
    pub current: Option<ScanSession>,
    pub undo: Option<ScanSession>,
}

struct Sub {
    id: u64,
    tx: mpsc::Sender<String>,
    approver: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    AllowSession,
    Deny,
}

#[derive(Default)]
struct Approvals {
    next: u64,
    pending: HashMap<u64, Option<Decision>>,
    session_allowed: HashSet<String>,
}

pub struct LaunchState {
    pub argv: Vec<String>,
    pub exit_code: Option<i32>,
    pub running: bool,
}

pub struct Daemon {
    pub target: RwLock<Option<Arc<Target>>>,
    pub scan: Mutex<ScanState>,
    pub progress: Arc<Progress>,
    pub busy: Mutex<Option<&'static str>>,
    pub table: Mutex<Table>,
    pub launches: Arc<Mutex<HashMap<i32, LaunchState>>>,
    pub policy: Mutex<Policy>,
    pub config: RwLock<Config>,
    subs: Mutex<Vec<Sub>>,
    next_sub: AtomicU64,
    approvals: Mutex<Approvals>,
    approvals_cv: Condvar,
    activity: Mutex<VecDeque<Value>>,
}

impl Daemon {
    pub fn new(policy: Policy, config: Config) -> Self {
        Self {
            target: RwLock::new(None),
            scan: Mutex::new(ScanState::default()),
            progress: Arc::new(Progress::default()),
            busy: Mutex::new(None),
            table: Mutex::new(Table::default()),
            launches: Arc::new(Mutex::new(HashMap::new())),
            policy: Mutex::new(policy),
            config: RwLock::new(config),
            subs: Mutex::new(Vec::new()),
            next_sub: AtomicU64::new(1),
            approvals: Mutex::new(Approvals::default()),
            approvals_cv: Condvar::new(),
            activity: Mutex::new(VecDeque::new()),
        }
    }

    /// Re-read settings that apply while running (timeouts, freeze interval).
    /// The startup policy is not touched: the live policy belongs to the window.
    pub fn reload_config(&self) {
        *self.config.write().unwrap() = Config::load();
    }

    pub fn access_mode(&self) -> &'static str {
        if process::has_ptrace_capability() {
            "capability"
        } else {
            match process::ptrace_scope() {
                Some(0) | None => "unrestricted",
                Some(3) => "disabled",
                _ => "children-only",
            }
        }
    }

    pub fn target(&self) -> Result<Arc<Target>, RpcError> {
        let t = self
            .target
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| RpcError::new("no_target", "no process attached").hint("call processes (query=...) and attach pid=..."))?;
        if !t.alive() {
            self.drop_target("exited");
            return Err(RpcError::new("process_gone", format!("{} (pid {}) has exited", t.name, t.pid))
                .hint("start the game again, then processes + attach"));
        }
        Ok(t)
    }

    pub fn drop_target(&self, reason: &str) {
        let old = self.target.write().unwrap().take();
        if let Some(t) = old {
            *self.scan.lock().unwrap() = ScanState::default();
            self.table.lock().unwrap().unfreeze_all();
            self.emit("target", json!({ "attached": false, "reason": reason, "name": t.name, "pid": t.pid }));
        }
    }

    // ---- events -----------------------------------------------------------

    pub fn subscribe(&self, tx: mpsc::Sender<String>, approver: bool) -> u64 {
        let id = self.next_sub.fetch_add(1, Ordering::Relaxed);
        self.subs.lock().unwrap().push(Sub { id, tx, approver });
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        self.subs.lock().unwrap().retain(|s| s.id != id);
        // A vanished approver must not leave agents waiting forever.
        if !self.has_approver() {
            let mut a = self.approvals.lock().unwrap();
            for d in a.pending.values_mut() {
                d.get_or_insert(Decision::Deny);
            }
            self.approvals_cv.notify_all();
        }
    }

    pub fn has_approver(&self) -> bool {
        self.subs.lock().unwrap().iter().any(|s| s.approver)
    }

    pub fn emit(&self, event: &str, data: Value) {
        let mut line = json!({ "event": event, "data": data }).to_string();
        line.push('\n');
        self.subs.lock().unwrap().retain(|s| s.tx.send(line.clone()).is_ok());
    }

    pub fn log_activity(&self, client: &str, tool: &str, summary: String, ok: bool) {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let entry = json!({ "ts": ts, "client": client, "tool": tool, "summary": summary, "ok": ok });
        {
            let mut a = self.activity.lock().unwrap();
            a.push_back(entry.clone());
            while a.len() > 300 {
                a.pop_front();
            }
        }
        self.emit("activity", entry);
    }

    pub fn recent_activity(&self, n: usize) -> Vec<Value> {
        let a = self.activity.lock().unwrap();
        a.iter().skip(a.len().saturating_sub(n)).cloned().collect()
    }

    // ---- approvals --------------------------------------------------------

    /// Gate for memory-changing calls. The GUI acts for the user and is trusted;
    /// every other client asks the user (or follows the configured policy).
    pub fn approve(&self, client: &str, tool: &str, summary: &str) -> Result<(), RpcError> {
        if client == GUI {
            return Ok(());
        }
        match *self.policy.lock().unwrap() {
            Policy::Allow => return Ok(()),
            Policy::Deny => {
                return Err(RpcError::new("approval_denied", "memory writes by agents are disabled (policy: deny)")
                    .hint("the user can change the policy in the Engram window"))
            }
            Policy::Ask => {}
        }
        let id = {
            let mut a = self.approvals.lock().unwrap();
            if a.session_allowed.contains(client) {
                return Ok(());
            }
            if !self.has_approver() {
                return Err(RpcError::new(
                    "approval_unavailable",
                    "this action needs the user's approval but the Engram window is not open",
                )
                .hint("ask the user to open Engram (engram-gui) or to start engramd with --policy allow"));
            }
            a.next += 1;
            let id = a.next;
            a.pending.insert(id, None);
            id
        };
        self.emit("approval_request", json!({ "id": id, "client": client, "tool": tool, "summary": summary }));
        let timeout = Duration::from_secs(self.config.read().unwrap().approval_timeout_s);
        let deadline = Instant::now() + timeout;
        let mut a = self.approvals.lock().unwrap();
        let decision = loop {
            if let Some(Some(d)) = a.pending.get(&id) {
                break Some(*d);
            }
            let now = Instant::now();
            if now >= deadline {
                break None;
            }
            a = self.approvals_cv.wait_timeout(a, deadline - now).unwrap().0;
        };
        a.pending.remove(&id);
        if decision == Some(Decision::AllowSession) {
            a.session_allowed.insert(client.to_string());
        }
        drop(a);
        let name = match decision {
            Some(Decision::Allow) => "allow",
            Some(Decision::AllowSession) => "allow_session",
            Some(Decision::Deny) => "deny",
            None => "timeout",
        };
        self.emit("approval_resolved", json!({ "id": id, "decision": name }));
        match decision {
            Some(Decision::Allow) | Some(Decision::AllowSession) => Ok(()),
            Some(Decision::Deny) => {
                Err(RpcError::new("approval_denied", "the user denied this action")
                    .hint("do not retry; ask the user what they want instead"))
            }
            None => Err(RpcError::new("approval_timeout", format!("the user did not answer within {} s", timeout.as_secs()))
                .hint("ask the user in the chat whether to proceed, then call again")),
        }
    }

    pub fn respond(&self, id: u64, d: Decision) -> bool {
        let mut a = self.approvals.lock().unwrap();
        match a.pending.get_mut(&id) {
            Some(slot) if slot.is_none() => {
                *slot = Some(d);
                self.approvals_cv.notify_all();
                true
            }
            _ => false,
        }
    }

    pub fn pending_approvals(&self) -> usize {
        self.approvals.lock().unwrap().pending.values().filter(|d| d.is_none()).count()
    }

    pub fn revoke_sessions(&self) {
        self.approvals.lock().unwrap().session_allowed.clear();
    }

    // ---- long operations --------------------------------------------------

    /// Run `f` as the single long operation, emitting progress events meanwhile.
    pub fn long_op<T>(&self, name: &'static str, f: impl FnOnce() -> Result<T, RpcError>) -> Result<T, RpcError> {
        {
            let mut b = self.busy.lock().unwrap();
            if let Some(cur) = *b {
                return Err(RpcError::new("busy", format!("a {cur} is running")).hint("wait for it, or call cancel"));
            }
            *b = Some(name);
        }
        // Clears `busy` even if the operation panics.
        struct Idle<'a>(&'a Mutex<Option<&'static str>>);
        impl Drop for Idle<'_> {
            fn drop(&mut self) {
                *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
        }
        let _idle = Idle(&self.busy);
        let done = AtomicBool::new(false);
        self.emit("progress", json!({ "op": name, "fraction": 0.0, "running": true }));
        let r = std::thread::scope(|s| {
            s.spawn(|| {
                while !done.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(200));
                    let p = &self.progress;
                    let (a, b) = (p.done.load(Ordering::Relaxed), p.total.load(Ordering::Relaxed).max(1));
                    self.emit("progress", json!({ "op": name, "fraction": (a as f64 / b as f64).min(1.0), "running": true }));
                }
            });
            struct Done<'a>(&'a AtomicBool);
            impl Drop for Done<'_> {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Relaxed);
                }
            }
            let _done = Done(&done);
            f()
        });
        drop(_idle);
        self.emit("progress", json!({ "op": name, "fraction": 1.0, "running": false }));
        r
    }
}
