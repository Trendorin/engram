//! `engram uninstall`: removes what `install.sh` installed, listed in its
//! manifest `<prefix>/share/engram/installed-files`, and optionally the user's
//! settings, tables, agent workspace and logs.

use crate::term::{bold, confirm, dim, ok, tilde, warn};
use engram_proto::client::Client;
use engram_proto::config::Config;
use engram_proto::paths;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub const MANIFEST: &str = "share/engram/installed-files";

/// Install prefix of the running binary (`<prefix>/bin/engram`).
fn prefix() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    Some(exe.parent()?.parent()?.to_path_buf())
}

fn read_manifest(prefix: &Path) -> Option<Vec<PathBuf>> {
    let text = std::fs::read_to_string(prefix.join(MANIFEST)).ok()?;
    // Only files inside the prefix: a damaged manifest must not reach elsewhere. The directory of
    // each entry is resolved first, so `..` and symlinked parents cannot lead out, and an install
    // made through a symlinked prefix still matches.
    Some(
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .filter_map(|p| {
                let name = p.file_name()?.to_owned();
                let dir = p.parent()?.canonicalize().ok()?;
                dir.starts_with(prefix).then(|| dir.join(name))
            })
            .collect(),
    )
}

/// Pids of our user's processes running exactly `binary`. Other copies of
/// Engram (another prefix, a cargo build) are left alone.
fn processes_of(binary: &Path) -> Vec<i32> {
    // SAFETY: getuid never fails.
    let uid = unsafe { libc::getuid() };
    let me = std::process::id() as i32;
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    let runs_binary = |pid: i32| {
        // `exe` is unreadable for an engine holding a capability; argv[0] is the
        // full path because Engram always starts its siblings by path.
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
        let exe = exe.map(|p| PathBuf::from(p.to_string_lossy().trim_end_matches(" (deleted)")));
        let argv0 = std::fs::read(format!("/proc/{pid}/cmdline"))
            .ok()
            .and_then(|c| c.split(|b| *b == 0).next().map(|a| PathBuf::from(String::from_utf8_lossy(a).into_owned())));
        exe.as_deref() == Some(binary) || argv0.as_deref() == Some(binary)
    };
    let owned = |pid: i32| {
        std::fs::read_to_string(format!("/proc/{pid}/status"))
            .ok()
            .and_then(|s| s.lines().find(|l| l.starts_with("Uid:")).and_then(|l| l.split_whitespace().nth(1)?.parse::<u32>().ok()))
            == Some(uid)
    };
    dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| *pid != me && owned(*pid) && runs_binary(*pid))
        .collect()
}

/// Stop this installation's window and engine. Returns how many were running.
fn stop_installed(prefix: &Path) -> usize {
    let gui = processes_of(&prefix.join("bin/engram-gui"));
    for pid in &gui {
        // SAFETY: asking our own window process to quit.
        unsafe { libc::kill(*pid, libc::SIGTERM) };
    }
    let engine = processes_of(&prefix.join("bin/engramd"));
    if !engine.is_empty() {
        if let Ok(mut c) = Client::connect("cli") {
            let _ = c.call("shutdown", json!({}));
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        for pid in processes_of(&prefix.join("bin/engramd")) {
            // SAFETY: stopping our own engine process.
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }
    gui.len() + engine.len()
}

fn remove_dir(label: &str, dir: &Path) {
    if dir.exists() {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => ok(label, tilde(dir)),
            Err(e) => warn(label, format!("{}: {e}", tilde(dir))),
        }
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let yes = args.iter().any(|a| a == "--yes" || a == "-y");
    let purge_flag = args.iter().any(|a| a == "--purge");
    let keep_flag = args.iter().any(|a| a == "--keep-data");

    let Some(prefix) = prefix() else {
        eprintln!("cannot determine where Engram is installed");
        return ExitCode::from(1);
    };
    let Some(files) = read_manifest(&prefix) else {
        if let Some(image) = paths::appimage() {
            eprintln!("This is an AppImage: delete {} to remove it.", image.display());
        } else if prefix == Path::new("/usr") {
            eprintln!("Engram was installed by your package manager. Remove it with it:");
            eprintln!("  sudo pacman -R engram   |   sudo apt remove engram   |   sudo dnf remove engram");
        } else {
            eprintln!("No install manifest at {}.", prefix.join(MANIFEST).display());
            eprintln!("This copy was not installed by install.sh (e.g. a cargo build); delete it by hand.");
        }
        return ExitCode::from(1);
    };

    println!("{}\n", bold("Uninstall Engram"));
    println!("  Installed in {}:", tilde(&prefix));
    for f in &files {
        println!("    {}", dim(&tilde(f)));
    }
    let user_dirs = [("settings", paths::config_dir()), ("tables & workspace", paths::data_dir()), ("logs", paths::state_dir())];
    println!();
    if !yes && !confirm("Remove Engram?", true) {
        println!("  Nothing changed.");
        return ExitCode::SUCCESS;
    }
    let purge = if purge_flag {
        true
    } else if keep_flag || yes {
        false
    } else {
        println!("  Your data: {}", user_dirs.iter().map(|(_, d)| tilde(d)).collect::<Vec<_>>().join(", "));
        confirm("Also delete your settings, saved address tables and logs?", false)
    };
    let custom_ws = Config::load().workspace;

    println!();
    if stop_installed(&prefix) > 0 {
        ok("stopped", "the running engine / window");
    }
    let mut failed = false;
    for f in &files {
        match std::fs::remove_file(f) {
            Ok(()) => ok("removed", tilde(f)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                warn("could not remove", format!("{}: {e}", tilde(f)));
                failed = true;
            }
        }
    }
    // Remove folders this left empty, but never the shared roots themselves.
    let keep: Vec<PathBuf> = ["", "bin", "share", "share/applications", "share/icons"].iter().map(|p| prefix.join(p)).collect();
    for f in &files {
        let mut dir = f.parent();
        while let Some(d) = dir.filter(|d| d.starts_with(&prefix) && !keep.contains(&d.to_path_buf())) {
            if std::fs::remove_dir(d).is_err() {
                break; // not empty
            }
            dir = d.parent();
        }
    }
    if purge {
        for (label, dir) in &user_dirs {
            remove_dir(label, dir);
        }
        remove_dir("runtime", &paths::runtime_dir());
        if !custom_ws.trim().is_empty() {
            warn("kept", format!("custom agent workspace {} (your folder)", custom_ws.trim()));
        }
    } else {
        println!("\n  {}", dim(&format!("Kept your data: {}", user_dirs.iter().map(|(_, d)| tilde(d)).collect::<Vec<_>>().join(", "))));
    }
    println!("\n  {}", dim("If you added Engram to an agent globally (e.g. `claude mcp add engram`), remove it there too."));
    if failed {
        ExitCode::from(1)
    } else {
        println!("\n  {}", bold("Engram is uninstalled."));
        ExitCode::SUCCESS
    }
}
