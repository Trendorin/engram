//! Target selection and process control: status, processes, attach, detach,
//! pause/resume and launching programs as children of the engine.

use super::{invalid, R};
use crate::daemon::{core_err, Daemon, Target};
use crate::params::P;
use engram_core::{hex, maps, process};
use engram_proto::{paths, RpcError};
use serde_json::{json, Value};
use std::sync::Arc;

pub(super) fn dispatch(d: &Daemon, client: &str, method: &str, p: &P) -> Option<R> {
    Some(match method {
        "status" => status(d),
        "processes" => processes(p),
        "attach" => attach(d, p),
        "detach" => {
            d.drop_target("detached");
            Ok(json!({ "detached": true }))
        }
        "process_control" => process_control(d, client, p),
        "launch" => launch(d, client, p),
        "launch_status" => launch_status(d, p),
        _ => return None,
    })
}

fn status(d: &Daemon) -> R {
    let target = d.target.read().unwrap().clone().filter(|t| t.alive());
    let st = d.scan.lock().unwrap();
    let busy = *d.busy.lock().unwrap();
    let scan = st.current.as_ref().map(|s| {
        let by_type: serde_json::Map<String, Value> = s.kinds.iter().zip(&s.counts).map(|(k, c)| (k.name(), json!(c))).collect();
        json!({
            "step": s.step,
            "last_compare": s.last_compare.name(),
            "total": s.total(),
            "by_type": by_type,
            "memory_mb": s.memory_bytes() / (1 << 20),
            "can_undo": st.undo.is_some(),
        })
    });
    let total = st.current.as_ref().map(|s| s.total());
    drop(st);
    let tab = d.table.lock().unwrap();
    let (entries, frozen) = (tab.entries.len(), tab.entries.iter().filter(|e| e.frozen.is_some()).count());
    drop(tab);
    let access = d.access_mode();

    let mut hints: Vec<String> = Vec::new();
    if access == "children-only" {
        hints.push("Engram can only read processes it launched (kernel Yama ptrace_scope=1 and no capability). Ask the user to run `engram setup` once.".into());
    }
    match (&target, total) {
        (None, _) => hints.push("No target. Call processes (query=<game name>) and attach pid=<pid>.".into()),
        (Some(_), None) if busy.is_none() => hints.push("Ask the user which value to find (and its current number), then scan value=<number>; for bars use scan compare=unknown type=f32.".into()),
        (Some(_), Some(0)) => hints.push("The last scan has 0 results: start over with scan (try another type or compare=unknown).".into()),
        (Some(_), Some(n)) if n > 20 => hints.push(format!("{n} candidates: ask the user to change the value in-game, then scan_next.")),
        (Some(_), Some(_)) => hints.push("Few candidates: verify (analyze/sample), then table_add with a description.".into()),
        _ => {}
    }

    Ok(json!({
        "engram": engram_proto::VERSION,
        "target": target.as_ref().map(|t| t.info()),
        "access": access,
        "busy": busy,
        "scan": scan,
        "table": { "entries": entries, "frozen": frozen },
        "policy": d.policy.lock().unwrap().name(),
        "gui_connected": d.has_approver(),
        "pending_approvals": d.pending_approvals(),
        "hints": hints,
    }))
}

fn own_uid() -> u32 {
    // SAFETY: getuid never fails.
    unsafe { libc::getuid() }
}

const SELF_NAMES: &[&str] = &["engramd", "engram", "engram-gui"];

fn processes(p: &P) -> R {
    let me = own_uid();
    let self_pid = std::process::id() as i32;
    let q = p.str("query").map(|s| s.to_lowercase()).filter(|s| !s.is_empty());
    let helpers = p.bool("include_helpers").unwrap_or(false);
    let limit = p.int_or("limit", 25, 1, 1000) as usize;
    let mut hidden = 0usize;
    let list: Vec<_> = process::list()
        .into_iter()
        .filter(|i| i.uid == me && i.accessible && i.pid != self_pid && !SELF_NAMES.contains(&i.name.as_str()))
        .filter(|i| q.as_ref().is_none_or(|q| i.name.to_lowercase().contains(q) || i.cmdline.to_lowercase().contains(q)))
        .filter(|i| {
            if i.helper && !helpers {
                hidden += 1;
                false
            } else {
                true
            }
        })
        .collect();
    Ok(json!({
        "total": list.len(),
        "hidden_helpers": hidden,
        "processes": list.iter().take(limit).collect::<Vec<_>>(),
    }))
}

fn attach(d: &Daemon, p: &P) -> R {
    let me = own_uid();
    let info = if let Some(pid) = p.int("pid") {
        process::info(pid as i32).ok_or_else(|| RpcError::new("not_found", format!("no process {pid}")))?
    } else if let Some(name) = p.str("name") {
        let n = name.to_lowercase();
        let mut cands: Vec<_> = process::list()
            .into_iter()
            .filter(|i| i.uid == me && i.accessible && !i.helper && i.name.to_lowercase().contains(&n))
            .collect();
        if cands.len() > 1 {
            let exact: Vec<_> = cands.iter().filter(|i| i.name.to_lowercase() == n).cloned().collect();
            if exact.len() == 1 {
                cands = exact;
            }
        }
        match cands.len() {
            0 => {
                return Err(RpcError::new("not_found", format!("no process matches '{name}'")).hint("call processes to see what is running"))
            }
            1 => cands.remove(0),
            _ => {
                let list: Vec<String> = cands.iter().take(8).map(|i| format!("{} pid={} {}MB", i.name, i.pid, i.rss_mb)).collect();
                return Err(RpcError::new("ambiguous", format!("several processes match '{name}': {}", list.join("; ")))
                    .hint("ask the user which one, then attach pid=<pid>"));
            }
        }
    } else {
        return Err(invalid("pass pid or name"));
    };
    if info.uid != me {
        return Err(RpcError::new("forbidden", format!("{} belongs to another user", info.name)));
    }
    if info.pid == std::process::id() as i32 {
        return Err(invalid("cannot attach to engramd itself"));
    }
    let t = Target::open(&info).map_err(|e| {
        if e.code == "permission_denied" && d.access_mode() == "children-only" {
            e.hint("The kernel only allows tracing child processes (Yama). Run `engram setup` once, or start the game with `engram launch -- <command>`.")
        } else {
            e
        }
    })?;
    let t = Arc::new(t);
    let (_, mods) = t.layout();
    let main = maps::find_module(&mods, &t.name).map(|m| json!({ "name": m.name, "base": hex(m.base), "size": hex(m.end - m.base) }));
    let previous_pid = d.target.read().unwrap().as_ref().map(|old| old.pid);
    *d.target.write().unwrap() = Some(t.clone());
    *d.scan.lock().unwrap() = Default::default();
    if previous_pid != Some(t.pid) {
        // Freezes were approved for one process; another instance of the same game must not inherit them.
        d.table.lock().unwrap().unfreeze_all();
    }
    let game_key = match &t.steam_app_id {
        Some(app) => format!("{}-{app}", t.name),
        None => t.name.clone(),
    };
    let loaded = d.table.lock().unwrap().switch_game(&game_key);
    let info_v = t.info();
    d.emit("target", json!({ "attached": true, "target": info_v }));
    d.emit("table", json!({}));
    Ok(json!({
        "attached": info_v,
        "modules": mods.len(),
        "main_module": main,
        "table_entries_loaded": loaded,
        "hint": "Ask the user what to find. Known number: scan value=<n>. Bar/unknown: scan compare=unknown type=f32.",
    }))
}

fn process_control(d: &Daemon, client: &str, p: &P) -> R {
    let t = d.target()?;
    let action = p.req_str("action")?;
    let sig = match action.as_str() {
        "pause" | "stop" => libc::SIGSTOP,
        "resume" | "continue" | "cont" => libc::SIGCONT,
        other => return Err(invalid(format!("unknown action '{other}' (pause|resume)"))),
    };
    if sig == libc::SIGSTOP {
        d.approve(client, "process_control", &format!("Pause {} (pid {})", t.name, t.pid))?;
    }
    // SAFETY: sending a signal to the attached process.
    if unsafe { libc::kill(t.pid, sig) } != 0 {
        return Err(core_err(std::io::Error::last_os_error().into()));
    }
    d.emit("target", json!({ "attached": true, "paused": sig == libc::SIGSTOP }));
    Ok(json!({ "ok": true, "paused": sig == libc::SIGSTOP }))
}

fn launch_status(d: &Daemon, p: &P) -> R {
    let pid = p.int("pid").ok_or_else(|| invalid("missing 'pid'"))? as i32;
    let l = d.launches.lock().unwrap();
    let s = l.get(&pid).ok_or_else(|| RpcError::new("not_found", format!("pid {pid} was not launched by engramd")))?;
    Ok(json!({ "pid": pid, "running": s.running, "exit_code": s.exit_code, "argv": s.argv }))
}

/// Client name of `engram launch`, the user's own wrapper (e.g. a Steam launch option).
const LAUNCHER: &str = "launcher";

fn launch(d: &Daemon, client: &str, p: &P) -> R {
    let argv: Vec<String> = match p.0.get("argv") {
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        Some(Value::String(s)) => s.split_whitespace().map(String::from).collect(),
        _ => Vec::new(),
    };
    if argv.is_empty() {
        return Err(invalid("argv must be a non-empty array"));
    }
    if client != LAUNCHER {
        d.approve(client, "launch", &format!("start the program: {}", argv.join(" ")))?;
    }
    let _ = std::fs::create_dir_all(paths::state_dir());
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::state_dir().join("launch.log"))
        .map_err(|e| core_err(e.into()))?;
    let mut cmd = std::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]).stdin(std::process::Stdio::null());
    cmd.stdout(log.try_clone().map_err(|e| core_err(e.into()))?).stderr(log);
    if let Some(cwd) = p.str("cwd") {
        cmd.current_dir(cwd);
    }
    if let Some(Value::Object(env)) = p.0.get("env") {
        cmd.env_clear();
        for (k, v) in env {
            if let Some(v) = v.as_str() {
                cmd.env(k, v);
            }
        }
    }
    // The child is forked *and* reaped by one dedicated thread using __WNOTHREAD.
    // A plain waitpid from any thread would also see ptrace stops of threads
    // traced by `watch` (the kernel scans the whole thread group) and steal them.
    let (tx, rx) = std::sync::mpsc::channel();
    let launches = d.launches.clone();
    let argv_t = argv.clone();
    std::thread::spawn(move || {
        let pid = match cmd.spawn() {
            Ok(child) => child.id() as i32,
            Err(e) => {
                let _ = tx.send(Err(e));
                return;
            }
        };
        launches.lock().unwrap().insert(pid, crate::daemon::LaunchState { argv: argv_t, exit_code: None, running: true });
        let _ = tx.send(Ok(pid));
        let mut st = 0;
        let code = loop {
            // SAFETY: waits only for our own child; no WUNTRACED, so only exits.
            let r = unsafe { libc::waitpid(pid, &mut st, libc::__WNOTHREAD) };
            if r == pid && (libc::WIFEXITED(st) || libc::WIFSIGNALED(st)) {
                break if libc::WIFEXITED(st) { Some(libc::WEXITSTATUS(st)) } else { Some(128 + libc::WTERMSIG(st)) };
            }
            if r == -1 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                break None;
            }
        };
        if let Some(s) = launches.lock().unwrap().get_mut(&pid) {
            s.running = false;
            s.exit_code = code;
        }
    });
    let pid = rx
        .recv()
        .map_err(|_| RpcError::new("launch_failed", "launcher thread died"))?
        .map_err(|e| RpcError::new("launch_failed", format!("{}: {e}", argv[0])))?;
    d.emit("launch", json!({ "pid": pid, "argv": argv }));
    Ok(
        json!({ "pid": pid, "hint": "When the game is up: processes query=<name> and attach. For Proton games attach to Game.exe, not the launcher." }),
    )
}
