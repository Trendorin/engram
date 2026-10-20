//! Process discovery with first-class Wine/Proton awareness.

use serde::Serialize;
use std::fs;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProcessKind {
    Native,
    Wine,
    Proton,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessInfo {
    pub pid: i32,
    /// Human name: `Game.exe` for Wine/Proton, the executable name otherwise.
    pub name: String,
    pub kind: ProcessKind,
    pub rss_mb: u64,
    pub ppid: i32,
    #[serde(skip)]
    pub uid: u32,
    /// Passes [`may_access`]: ours, not setuid/setgid/file-capability, dumpable.
    #[serde(skip)]
    pub accessible: bool,
    #[serde(skip)]
    pub start_time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    pub cmdline: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steam_app_id: Option<String>,
    /// Runtime plumbing (wineserver, services.exe, pressure-vessel, ...), hidden by default.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub helper: bool,
}

const WINE_HELPERS: &[&str] = &[
    "wineserver",
    "services.exe",
    "winedevice.exe",
    "plugplay.exe",
    "explorer.exe",
    "rpcss.exe",
    "svchost.exe",
    "tabtip.exe",
    "conhost.exe",
    "start.exe",
    "rundll32.exe",
    "steam.exe",
    "winedbg.exe",
    "wine64-preloader",
    "wine-preloader",
    "wine",
    "wine64",
    "crashpad_handler.exe",
    "unitycrashhandler64.exe",
    "unitycrashhandler32.exe",
];
const LINUX_HELPERS: &[&str] = &[
    "reaper",
    "pressure-vessel-wrap",
    "pv-bwrap",
    "srt-bwrap",
    "pressure-vessel-adverb",
    "steam-runtime-launcher-service",
    "steam-runtime-supervisor",
    "proton",
    "steam-launch-wrapper",
];

/// (state, ppid, start_time in clock ticks since boot)
pub fn read_stat(pid: i32) -> Option<(char, i32, u64)> {
    let s = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm may contain spaces and parentheses; fields resume after the last ')'.
    let rest = &s[s.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some((f.first()?.chars().next()?, f.get(1)?.parse().ok()?, f.get(19)?.parse().ok()?))
}

pub fn real_uid(pid: i32) -> Option<u32> {
    let s = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status_ids(&s, "Uid:")?.first().copied()
}

/// The numbers after `key` in /proc/<pid>/status ("Uid:", "Gid:": real, effective, saved, fs).
fn status_ids(status: &str, key: &str) -> Option<Vec<u32>> {
    let line = status.lines().find(|l| l.starts_with(key))?;
    line[key.len()..].split_whitespace().map(|v| v.parse().ok()).collect()
}

fn status_caps(status: &str, key: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with(key))?;
    u64::from_str_radix(line[key.len()..].trim(), 16).ok()
}

/// Would the kernel let an unprivileged process of our user ptrace `pid`?
///
/// This mirrors `ptrace_may_access()` without the CAP_SYS_PTRACE bypass, so the
/// capability `engram setup` grants only lifts Yama's "children only" rule:
/// every uid of the target is ours, every gid is one of our groups, it holds no
/// capability we lack, and it is dumpable. That rules out setuid/setgid and
/// file-capability programs (sudo, passwd, newuidmap) and processes that made
/// themselves non-dumpable to keep secrets (ssh-agent, gpg-agent).
pub fn may_access(pid: i32) -> bool {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: these calls cannot fail.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) else { return false };
    let Ok(own) = fs::read_to_string("/proc/self/status") else { return false };
    let groups = own_groups();
    let uids_ok = status_ids(&status, "Uid:").is_some_and(|v| v.len() == 4 && v.iter().all(|u| *u == uid));
    let gids_ok = status_ids(&status, "Gid:").is_some_and(|v| v.len() == 4 && v.iter().all(|g| *g == gid || groups.contains(g)));
    let caps_ok = match (status_caps(&status, "CapPrm:"), status_caps(&own, "CapPrm:")) {
        (Some(theirs), Some(ours)) => theirs & !ours == 0,
        _ => false,
    };
    // The kernel hands /proc/<pid>/mem to root while the process is not dumpable.
    let dumpable = fs::metadata(format!("/proc/{pid}/mem")).is_ok_and(|m| m.uid() == uid);
    uids_ok && gids_ok && caps_ok && dumpable
}

fn own_groups() -> Vec<u32> {
    // SAFETY: a first call with size 0 returns the count; the second fills `n` entries.
    unsafe {
        let n = libc::getgroups(0, std::ptr::null_mut());
        if n <= 0 {
            return Vec::new();
        }
        let mut v = vec![0 as libc::gid_t; n as usize];
        let n = libc::getgroups(n, v.as_mut_ptr());
        v.truncate(n.max(0) as usize);
        v
    }
}

/// True if `pid` still refers to the process that had `start_time` (pids are reused).
pub fn is_same(pid: i32, start_time: u64) -> bool {
    matches!(read_stat(pid), Some((state, _, st)) if st == start_time && state != 'Z')
}

fn basename(s: &str) -> &str {
    s.rsplit(['/', '\\']).next().unwrap_or(s)
}

fn is_exe(s: &str) -> bool {
    s.to_ascii_lowercase().ends_with(".exe")
}

pub fn info(pid: i32) -> Option<ProcessInfo> {
    let (_, ppid, start_time) = read_stat(pid)?;
    let raw = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if raw.is_empty() {
        return None; // kernel thread or zombie
    }
    let argv: Vec<String> = raw.split(|b| *b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
    let comm = fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim().to_string();
    let exe = fs::read_link(format!("/proc/{pid}/exe")).ok().map(|p| p.to_string_lossy().into_owned());
    let exe_base = exe.as_deref().map(basename).unwrap_or("");

    let a0 = argv.first().map(|s| basename(s)).unwrap_or("");
    let name = if is_exe(a0) {
        a0.to_string()
    } else if exe_base.starts_with("wine") && argv.len() > 1 && is_exe(&argv[1]) {
        basename(&argv[1]).to_string()
    } else if !exe_base.is_empty() {
        exe_base.to_string()
    } else {
        comm.clone()
    };

    let wine = is_exe(&name) || exe_base.starts_with("wine") || is_exe(&comm);
    let accessible = may_access(pid);
    // With CAP_SYS_PTRACE we could read anyone's environment; only look at our own.
    let env = if accessible { fs::read(format!("/proc/{pid}/environ")).unwrap_or_default() } else { Vec::new() };
    let env_get = |key: &str| -> Option<String> {
        let prefix = format!("{key}=");
        env.split(|b| *b == 0).find(|e| e.starts_with(prefix.as_bytes())).map(|e| String::from_utf8_lossy(&e[prefix.len()..]).into_owned())
    };
    let steam_app_id = env_get("SteamAppId").or_else(|| env_get("SteamGameId")).filter(|v| !v.is_empty() && v != "0");
    let kind = if !wine {
        ProcessKind::Native
    } else if env_get("STEAM_COMPAT_DATA_PATH").is_some() || env_get("PROTON_VERSION").is_some() {
        ProcessKind::Proton
    } else {
        ProcessKind::Wine
    };
    let lname = name.to_ascii_lowercase();
    let helper = WINE_HELPERS.contains(&lname.as_str()) || LINUX_HELPERS.contains(&lname.as_str());

    let rss_mb = fs::read_to_string(format!("/proc/{pid}/statm"))
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map(|pages| pages * 4096 / (1024 * 1024))
        .unwrap_or(0);

    let mut cmdline = argv.join(" ");
    if cmdline.len() > 160 {
        let cut = (0..=160).rev().find(|i| cmdline.is_char_boundary(*i)).unwrap_or(0);
        cmdline.truncate(cut);
        cmdline.push('…');
    }

    Some(ProcessInfo { pid, name, kind, rss_mb, ppid, uid: real_uid(pid)?, accessible, start_time, exe, cmdline, steam_app_id, helper })
}

/// All user-space processes, largest resident memory first (games float to the top).
pub fn list() -> Vec<ProcessInfo> {
    let mut out: Vec<ProcessInfo> = fs::read_dir("/proc")
        .map(|rd| rd.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<i32>().ok()).filter_map(info).collect())
        .unwrap_or_default();
    out.sort_by(|a, b| b.rss_mb.cmp(&a.rss_mb).then(a.pid.cmp(&b.pid)));
    out
}

pub fn threads(pid: i32) -> Vec<i32> {
    fs::read_dir(format!("/proc/{pid}/task"))
        .map(|rd| rd.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok()).collect())
        .unwrap_or_default()
}

/// Was CAP_SYS_PTRACE granted to us (`engram setup`)? It is kept permitted but
/// not effective; see [`crate::caps`].
pub fn has_ptrace_capability() -> bool {
    crate::caps::ptrace_permitted()
}

pub fn ptrace_scope() -> Option<u32> {
    fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope").ok()?.trim().parse().ok()
}

/// Is `pid` a descendant of `ancestor`? (Yama scope 1 allows tracing descendants.)
pub fn is_descendant(pid: i32, ancestor: i32) -> bool {
    let mut cur = pid;
    for _ in 0..64 {
        match read_stat(cur) {
            Some((_, ppid, _)) if ppid == ancestor => return true,
            Some((_, ppid, _)) if ppid > 1 => cur = ppid,
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_follows_ownership_and_dumpability() {
        let me = std::process::id() as i32;
        assert!(may_access(me));
        // SAFETY: getuid cannot fail.
        if unsafe { libc::getuid() } == 0 {
            return; // as root every process "belongs" to us; nothing to tell apart
        }
        assert!(!may_access(1), "init belongs to root");
        // A child that made itself non-dumpable, like ssh-agent does. Between fork and
        // _exit the child only makes async-signal-safe calls.
        // SAFETY: fork, then prctl/pause/_exit in the child; the parent kills and reaps it.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            unsafe {
                libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
                libc::pause();
                libc::_exit(0);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
        let ok = may_access(pid);
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            libc::waitpid(pid, std::ptr::null_mut(), 0);
        }
        assert!(!ok, "a non-dumpable process must be refused");
    }
}
