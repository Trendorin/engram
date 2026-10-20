//! Filesystem locations (XDG).

use std::path::PathBuf;

fn uid() -> u32 {
    // SAFETY: getuid never fails.
    unsafe { libc::getuid() }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(fallback)).join("engram")
}

/// Private per-user runtime directory (0700) holding the socket and lock.
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_absolute()) {
        Some(p) => p.join("engram"),
        None => PathBuf::from(format!("/tmp/engram-{}", uid())),
    }
}

pub fn socket() -> PathBuf {
    std::env::var_os("ENGRAM_SOCKET").map(PathBuf::from).unwrap_or_else(|| runtime_dir().join("engramd.sock"))
}

pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state")
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

pub fn tables_dir() -> PathBuf {
    data_dir().join("tables")
}

/// Default folder an agent terminal is opened in (holds AGENTS.md + MCP config).
pub fn default_agent_workspace() -> PathBuf {
    data_dir().join("workspace")
}

/// The agent workspace, honouring the `workspace` setting.
pub fn agent_workspace() -> PathBuf {
    let custom = crate::config::Config::load().workspace;
    if custom.trim().is_empty() {
        default_agent_workspace()
    } else {
        expand_home(custom.trim())
    }
}

/// `~/x` → `$HOME/x`.
pub fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if p == "~" => home(),
        None => PathBuf::from(p),
    }
}

/// The AppImage we run from, if any. `$APPIMAGE` alone is not proof: every program
/// started from *any* AppImage inherits it, so our executable must also be inside
/// the mounted `$APPDIR`.
pub fn appimage() -> Option<PathBuf> {
    let image = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file())?;
    let root = std::env::var_os("APPDIR").map(PathBuf::from)?.canonicalize().ok()?;
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.starts_with(&root).then_some(image)
}

/// What agents and scripts should run to reach the `engram` CLI: the AppImage
/// itself (with arguments it runs the CLI), or the `engram` binary.
pub fn cli_command() -> PathBuf {
    appimage().or_else(|| sibling_binary("engram").and_then(|p| p.canonicalize().ok())).unwrap_or_else(|| PathBuf::from("engram"))
}

/// Locate a sibling binary (`engramd`, `engram`): next to the current executable,
/// then on PATH.
pub fn sibling_binary(name: &str) -> Option<PathBuf> {
    let env_key = format!("{}_BIN", name.to_ascii_uppercase().replace('-', "_"));
    if let Some(p) = std::env::var_os(env_key).map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    std::env::var_os("PATH").and_then(|path| std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file()))
}
