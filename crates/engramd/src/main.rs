//! engramd — the Engram engine.
//!
//! Holds the one debugging session (target, scan, address table, freezes) and
//! serves it over a private Unix socket to the GUI, the CLI and MCP agents.
//! It is the only Engram binary that may carry CAP_SYS_PTRACE.

mod daemon;
mod params;
mod table;
mod target;
mod tools;

use daemon::{Daemon, Policy};
use engram_proto::config::Config;
use engram_proto::{paths, Request, Response, RpcError};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{mpsc, Arc};

fn main() {
    // Before any thread exists: the capability is raised only around attach.
    engram_core::caps::lower();
    // Policy precedence: --policy, then $ENGRAM_POLICY, then the `policy` setting.
    let config = Config::load();
    let mut policy = std::env::var("ENGRAM_POLICY")
        .ok()
        .and_then(|p| Policy::parse(&p))
        .or_else(|| Policy::parse(&config.policy))
        .unwrap_or(Policy::Ask);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--policy" => policy = args.next().and_then(|p| Policy::parse(&p)).unwrap_or_else(|| die("--policy ask|allow|deny")),
            "--version" | "-V" => {
                println!("engramd {}", engram_proto::VERSION);
                return;
            }
            "--help" | "-h" => {
                println!("engramd [--policy ask|allow|deny]\nUsually started automatically by `engram` or the GUI.");
                return;
            }
            other => die(&format!("unknown argument {other}")),
        }
    }

    let dir = paths::runtime_dir();
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| die(&format!("cannot create {}: {e}", dir.display())));
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));

    // Single instance: hold an exclusive lock for our lifetime.
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("engramd.lock"))
        .unwrap_or_else(|e| die(&format!("lock: {e}")));
    // SAFETY: flock on a file we own.
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        eprintln!("engramd is already running");
        return;
    }

    let sock = paths::socket();
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap_or_else(|e| die(&format!("bind {}: {e}", sock.display())));
    let _ = std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600));

    let d = Arc::new(Daemon::new(policy, config));
    eprintln!(
        "engramd {} listening on {} (pid {}, access: {}, policy: {})",
        engram_proto::VERSION,
        sock.display(),
        std::process::id(),
        d.access_mode(),
        policy.name()
    );
    table::spawn_freezer(d.clone());

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if !peer_is_us(&stream) {
            continue; // the socket is 0600 in a 0700 dir; this is defence in depth
        }
        let d = d.clone();
        std::thread::spawn(move || serve(d, stream));
    }
}

fn die(msg: &str) -> ! {
    eprintln!("engramd: {msg}");
    std::process::exit(2)
}

fn peer_is_us(s: &UnixStream) -> bool {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: SO_PEERCRED fills a ucred of `len` bytes.
    let r = unsafe { libc::getsockopt(s.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut _, &mut len) };
    r == 0 && cred.uid == unsafe { libc::getuid() }
}

fn serve(d: Arc<Daemon>, stream: UnixStream) {
    let Ok(read_half) = stream.try_clone() else { return };
    let reader = BufReader::new(read_half);
    let mut writer = stream;
    for line in reader.lines() {
        let Ok(line) = line else { return };
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response { id: 0, ok: false, result: None, error: Some(RpcError::new("protocol_error", e.to_string())) };
                if send(&mut writer, &resp).is_err() {
                    return;
                }
                continue;
            }
        };
        let client = req.client.clone().unwrap_or_else(|| "unknown".into());

        if req.method == "subscribe" {
            let approver = client == "gui" && req.params.get("approver").and_then(|v| v.as_bool()).unwrap_or(false);
            let (tx, rx) = mpsc::channel::<String>();
            let resp = Response { id: req.id, ok: true, result: Some(serde_json::json!({ "subscribed": true })), error: None };
            if send(&mut writer, &resp).is_err() {
                return;
            }
            let sub_id = d.subscribe(tx, approver);
            for msg in rx {
                if writer.write_all(msg.as_bytes()).is_err() {
                    break;
                }
            }
            d.unsubscribe(sub_id);
            return;
        }

        let result = tools::handle(&d, &client, &req.method, &req.params);
        let resp = match result {
            Ok(v) => Response { id: req.id, ok: true, result: Some(v), error: None },
            Err(e) => Response { id: req.id, ok: false, result: None, error: Some(e) },
        };
        if send(&mut writer, &resp).is_err() {
            return;
        }
    }
}

fn send(w: &mut UnixStream, resp: &Response) -> std::io::Result<()> {
    let mut s = serde_json::to_string(resp).unwrap_or_else(|_| "{}".into());
    s.push('\n');
    w.write_all(s.as_bytes())
}
