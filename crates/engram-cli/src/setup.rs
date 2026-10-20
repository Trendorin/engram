//! `doctor`, `setup` and `agent-init`.

use crate::term::{bad, bold, ok, tilde, warn};
use engram_proto::client::Client;
use engram_proto::{agentkit, paths};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn has_file_cap(p: &Path) -> bool {
    let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) else { return false };
    // SAFETY: probing the size of an xattr; no buffer is written.
    unsafe { libc::getxattr(c.as_ptr(), c"security.capability".as_ptr(), std::ptr::null_mut(), 0) > 0 }
}

fn engram_bin() -> PathBuf {
    paths::appimage().or_else(|| std::env::current_exe().and_then(|p| p.canonicalize()).ok()).unwrap_or_else(|| PathBuf::from("engram"))
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|p| p.is_file()))
}

pub fn doctor() -> ExitCode {
    println!("{}\n", bold(&format!("Engram {} doctor", engram_proto::VERSION)));
    let mut healthy = true;

    let daemon_bin = paths::sibling_binary("engramd");
    match &daemon_bin {
        Some(p) => ok("engramd binary", p.display().to_string()),
        None => {
            bad("engramd binary", "not found next to engram or on PATH");
            healthy = false;
        }
    }
    match paths::sibling_binary("engram-gui") {
        Some(p) => ok("GUI", p.display().to_string()),
        None => warn("GUI", "engram-gui not found (CLI/agents still work)"),
    }

    let scope = std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope").ok().map(|s| s.trim().to_string());
    let cap = daemon_bin.as_deref().is_some_and(has_file_cap);
    match (scope.as_deref(), cap) {
        (_, true) => ok("memory access", "engramd has CAP_SYS_PTRACE (any of your processes)"),
        (Some("0") | None, _) => ok("memory access", "kernel allows tracing your processes (ptrace_scope=0)"),
        (Some("3"), _) => {
            bad("memory access", "ptrace disabled by the kernel (ptrace_scope=3)");
            healthy = false;
        }
        (Some(s), false) => {
            warn("memory access", format!("ptrace_scope={s}: only programs started via `engram launch`. Run `engram setup` to fix."));
        }
    }

    match Client::connect("cli") {
        Ok(mut c) => match c.call("status", json!({})) {
            Ok(s) => {
                ok(
                    "daemon",
                    format!("running, access={}, policy={}", s["access"].as_str().unwrap_or("?"), s["policy"].as_str().unwrap_or("?")),
                );
                match s["target"].get("name") {
                    Some(n) => ok("target", format!("{} (pid {})", n.as_str().unwrap_or("?"), s["target"]["pid"])),
                    None => warn("target", "none attached"),
                }
                if s["gui_connected"] == json!(true) {
                    ok("approvals", "Engram window is open and will ask before agent writes");
                } else {
                    warn("approvals", "Engram window not open: agent writes are refused until it is (policy=ask)");
                }
            }
            Err(e) => {
                bad("daemon", e.to_string());
                healthy = false;
            }
        },
        Err(_) => warn("daemon", "not running (starts automatically on first use)"),
    }

    let agents: Vec<&str> = ["opencode", "claude", "codex", "gemini"].into_iter().filter(|a| on_path(a).is_some()).collect();
    if agents.is_empty() {
        warn("agents", "no opencode/claude/codex/gemini on PATH (MCP: `engram mcp`)");
    } else {
        ok("agents", agents.join(", "));
    }
    let ws = paths::agent_workspace();
    if ws.join("AGENTS.md").is_file() {
        ok("agent workspace", tilde(&ws));
    } else {
        warn("agent workspace", format!("not prepared; `engram agent-init {}`", tilde(&ws)));
    }
    let cfg = paths::config_file();
    ok("settings", if cfg.is_file() { tilde(&cfg) } else { "defaults (`engram config` to change)".into() });

    println!();
    if healthy {
        println!("Ready. Open the window (engram-gui) and start an agent in {}", tilde(&ws));
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

pub fn setup() -> ExitCode {
    let Some(daemon) = paths::sibling_binary("engramd").and_then(|p| p.canonicalize().ok()) else {
        bad("engramd", "not found next to engram or on PATH");
        return ExitCode::from(1);
    };
    println!("{}\n", bold("Engram setup"));
    if paths::appimage().is_some() {
        warn("memory access", "an AppImage is read-only, so its engine cannot be given CAP_SYS_PTRACE");
        println!("  Start the game from Engram instead (\"Start a program\" in the window, or");
        println!("  `engram launch -- <command>`, e.g. as the Steam launch option), or install");
        println!("  the Arch, Debian or Fedora package to grant access once.\n");
    } else if has_file_cap(&daemon) {
        ok("memory access", format!("{} already has CAP_SYS_PTRACE", tilde(&daemon)));
    } else {
        println!("  Linux lets a program read another program's memory only if it started it");
        println!("  (Yama ptrace_scope). Engram grants exactly one capability, CAP_SYS_PTRACE,");
        println!("  to its small engine binary {}.", tilde(&daemon));
        println!("  The window and agents stay unprivileged; the engine serves only your user");
        println!("  over a private socket and refuses processes of other users.\n");
        let setcap = ["/usr/bin/setcap", "/usr/sbin/setcap", "/sbin/setcap"].into_iter().find(|p| Path::new(p).exists());
        let Some(setcap) = setcap else {
            bad("setcap", "not found (install libcap / libcap2-bin)");
            return ExitCode::from(1);
        };
        let elevate = if on_path("pkexec").is_some() { "pkexec" } else { "sudo" };
        let status = Command::new(elevate).arg(setcap).arg("cap_sys_ptrace=ep").arg(&daemon).status();
        match status {
            Ok(s) if s.success() && has_file_cap(&daemon) => ok("memory access", "granted"),
            _ => {
                bad("memory access", format!("not granted. Manually: sudo setcap cap_sys_ptrace=ep {}", daemon.display()));
                return ExitCode::from(1);
            }
        }
        if let Ok(mut c) = Client::connect("cli") {
            let _ = c.call("shutdown", json!({}));
            ok("engine", "restarted (it starts again on next use)");
        }
    }
    let ws = paths::agent_workspace();
    match agentkit::init_workspace(&ws, &engram_bin()) {
        Ok(_) => ok("agent workspace", tilde(&ws)),
        Err(e) => warn("agent workspace", e.to_string()),
    }
    println!("\n  Reinstalling engramd drops the capability; run `engram setup` again afterwards.");
    ExitCode::SUCCESS
}

pub fn agent_init(dir: Option<&str>) -> ExitCode {
    let dir = dir.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| paths::agent_workspace()));
    match agentkit::init_workspace(&dir, &engram_bin()) {
        Ok(files) => {
            for f in files {
                println!("wrote {}", f.display());
            }
            println!("\nStart your agent here, e.g.:  cd {} && opencode", dir.display());
            println!("It reads AGENTS.md and connects to Engram over MCP (opencode.json / .mcp.json).");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("agent-init failed: {e}");
            ExitCode::from(1)
        }
    }
}
