//! Opening an agent terminal next to the window, opening folders, and
//! `engram setup`.

use crate::format;
use engram_proto::config::Config;
use engram_proto::{agentkit, paths};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Agents Engram knows how to start: (label, command, client name in the activity feed).
const AGENTS: &[(&str, &str, &str)] = &[
    ("OpenCode", "opencode", "opencode"),
    ("Claude Code", "claude", "claude-code"),
    ("Codex", "codex", "codex"),
    ("Gemini CLI", "gemini", "gemini"),
];
pub const SHELL: &str = "Shell only";

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|p| p.is_file()))
}

/// Installed agents first, then the plain shell.
pub fn available_agents() -> Vec<String> {
    let mut v: Vec<String> = AGENTS.iter().filter(|(_, cmd, _)| on_path(cmd).is_some()).map(|(l, _, _)| l.to_string()).collect();
    v.push(SHELL.to_string());
    v
}

fn engram_bin() -> PathBuf {
    paths::cli_command()
}

const TERMINALS: &[&str] =
    &["konsole", "gnome-terminal", "kgx", "ptyxis", "kitty", "alacritty", "ghostty", "wezterm", "foot", "xfce4-terminal", "tilix", "xterm"];

fn exists(program: &str) -> bool {
    on_path(program).is_some() || Path::new(program).is_file()
}

/// First installed terminal from $TERMINAL and the known list (ignores the setting).
pub fn detect_terminal() -> Option<String> {
    std::env::var("TERMINAL").ok().into_iter().chain(TERMINALS.iter().map(|s| s.to_string())).find(|t| exists(t))
}

/// Terminal emulator plus the arguments that run `cmd` in `dir`. The
/// `terminal` setting may carry its own arguments ("kitty --single-instance").
fn terminal_command(dir: &Path, cmd: Option<&str>) -> Option<Command> {
    let d = dir.display().to_string();
    let configured = format::split_command(&Config::load().terminal);
    let (term, extra) = match configured.split_first() {
        Some((t, rest)) if exists(t) => (t.clone(), rest.to_vec()),
        _ => (detect_terminal()?, Vec::new()),
    };
    let name = Path::new(&term).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    let mut c = Command::new(&term);
    c.args(extra);
    c.current_dir(dir);
    match (name.as_str(), cmd) {
        ("konsole", _) => {
            c.args(["--workdir", &d]);
            if let Some(cmd) = cmd {
                c.args(["-e", cmd]);
            }
        }
        ("gnome-terminal" | "kgx" | "ptyxis", _) => {
            c.arg(format!("--working-directory={d}"));
            if let Some(cmd) = cmd {
                c.args(["--", cmd]);
            }
        }
        ("wezterm", _) => {
            c.args(["start", "--cwd", &d]);
            if let Some(cmd) = cmd {
                c.args(["--", cmd]);
            }
        }
        ("kitty", _) => {
            c.args(["--directory", &d]);
            if let Some(cmd) = cmd {
                c.arg(cmd);
            }
        }
        ("foot", _) => {
            c.args(["-D", &d]);
            if let Some(cmd) = cmd {
                c.arg(cmd);
            }
        }
        ("ghostty", _) => {
            c.arg(format!("--working-directory={d}"));
            if let Some(cmd) = cmd {
                c.args(["-e", cmd]);
            }
        }
        ("alacritty" | "xfce4-terminal" | "tilix", _) => {
            c.args(["--working-directory", &d]);
            if let Some(cmd) = cmd {
                c.args(["-e", cmd]);
            }
        }
        (_, Some(cmd)) => {
            c.args(["-e", cmd]);
        }
        (_, None) => {}
    }
    Some(c)
}

/// Prepare the agent workspace and open a terminal running `agent` in it.
/// Remembers the choice as the `agent` setting. Returns a status-bar message.
pub fn open_agent_terminal(agent: &str) -> Result<String, String> {
    let mut cfg = Config::load();
    if cfg.agent != agent && cfg.set("agent", agent).is_ok() {
        let _ = cfg.save();
    }
    let ws = paths::agent_workspace();
    agentkit::init_workspace(&ws, &engram_bin()).map_err(|e| format!("cannot prepare {}: {e}", ws.display()))?;
    let known = AGENTS.iter().find(|(l, _, _)| *l == agent);
    let cmd = known.map(|(_, c, _)| *c);
    let mut term = terminal_command(&ws, cmd).ok_or_else(|| "no terminal emulator found; set $TERMINAL".to_string())?;
    term.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some((_, _, client)) = known {
        term.env("ENGRAM_CLIENT", client);
    }
    let mut child = term.spawn().map_err(|e| format!("cannot start the terminal: {e}"))?;
    // Reap it when it exits so no zombie is left behind.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(match cmd {
        Some(_) => crate::i18n::tf("Opened {} in {}", &[&agent, &format::tilde(&ws)]),
        None => crate::i18n::tf("Opened a terminal in {} — start any agent there", &[&format::tilde(&ws)]),
    })
}

/// Show a folder in the file manager.
pub fn open_folder(path: &str) -> Result<String, String> {
    let dir = paths::expand_home(path);
    if !dir.exists() {
        return Err(format!("{path} does not exist yet"));
    }
    let mut child = Command::new("xdg-open")
        .arg(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot run xdg-open: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(crate::i18n::tf("Opened {}", &[&path]))
}

/// Run `engram setup` (asks for the password through pkexec). Blocking.
pub fn grant_access() -> Result<String, String> {
    let bin = paths::sibling_binary("engram").ok_or_else(|| "the engram binary was not found next to engram-gui or on PATH".to_string())?;
    let out = Command::new(bin).arg("setup").stdin(Stdio::null()).output().map_err(|e| format!("cannot run engram setup: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        Ok(crate::i18n::t("Access granted: engramd can now read your programs (the engine restarts)"))
    } else {
        let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("engram setup failed").trim().to_string();
        Err(last)
    }
}
