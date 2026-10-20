//! Blocking daemon client used by the CLI, the MCP server and the GUI.

use crate::{paths, Event, Request, Response, RpcError};
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
    name: String,
}

impl Client {
    pub fn connect(name: &str) -> io::Result<Self> {
        let s = UnixStream::connect(paths::socket())?;
        // Anything requests carry (arguments, the launcher's environment) goes to this socket:
        // be sure that it is our own engine, not a socket another user put at a guessable path.
        if !peer_is_us(&s) {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the engine socket belongs to another user"));
        }
        Ok(Self { reader: BufReader::new(s.try_clone()?), writer: s, next_id: 1, name: name.to_string() })
    }

    /// Connect, starting `engramd` in the background if it is not running.
    pub fn connect_or_spawn(name: &str) -> Result<Self, RpcError> {
        if let Ok(c) = Self::connect(name) {
            return Ok(c);
        }
        spawn_daemon()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match Self::connect(name) {
                Ok(c) => return Ok(c),
                Err(e) if Instant::now() > deadline => {
                    return Err(RpcError::new("daemon_unavailable", format!("engramd did not start: {e}"))
                        .hint(format!("see {}", paths::state_dir().join("engramd.log").display())))
                }
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next_id;
        self.next_id += 1;
        let req = Request { id, method: method.to_string(), params, client: Some(self.name.clone()) };
        let io_err = |e: io::Error| RpcError::new("daemon_unavailable", e.to_string());
        let mut line = serde_json::to_string(&req).expect("request serializes");
        line.push('\n');
        self.writer.write_all(line.as_bytes()).map_err(io_err)?;
        let mut buf = String::new();
        loop {
            buf.clear();
            if self.reader.read_line(&mut buf).map_err(io_err)? == 0 {
                return Err(RpcError::new("daemon_unavailable", "daemon closed the connection"));
            }
            let resp: Response = serde_json::from_str(&buf).map_err(|e| RpcError::new("protocol_error", e.to_string()))?;
            if resp.id != id {
                continue;
            }
            return if resp.ok {
                Ok(resp.result.unwrap_or(Value::Null))
            } else {
                Err(resp.error.unwrap_or_else(|| RpcError::new("unknown", "request failed")))
            };
        }
    }

    /// Turn this connection into an event stream. `approver` marks the
    /// subscriber as able to answer write-approval requests (the GUI).
    pub fn subscribe(mut self, approver: bool) -> Result<EventStream, RpcError> {
        self.call("subscribe", serde_json::json!({ "approver": approver }))?;
        Ok(EventStream { reader: self.reader })
    }
}

pub struct EventStream {
    reader: BufReader<UnixStream>,
}

impl Iterator for EventStream {
    type Item = Event;
    fn next(&mut self) -> Option<Event> {
        let mut buf = String::new();
        loop {
            buf.clear();
            if self.reader.read_line(&mut buf).ok()? == 0 {
                return None;
            }
            if let Ok(e) = serde_json::from_str::<Event>(&buf) {
                return Some(e);
            }
        }
    }
}

pub fn spawn_daemon() -> Result<(), RpcError> {
    let bin = paths::sibling_binary("engramd").ok_or_else(|| {
        RpcError::new("daemon_unavailable", "engramd binary not found").hint("install Engram or put engramd next to engram / on PATH")
    })?;
    let log_dir = paths::state_dir();
    let _ = std::fs::create_dir_all(&log_dir);
    let log = std::fs::OpenOptions::new().create(true).append(true).open(log_dir.join("engramd.log")).ok();
    let mut cmd = Command::new(bin);
    cmd.stdin(Stdio::null());
    match log {
        Some(f) => {
            cmd.stdout(f.try_clone().map(Stdio::from).unwrap_or_else(|_| Stdio::null()));
            cmd.stderr(Stdio::from(f));
        }
        None => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }
    // SAFETY: setsid is async-signal-safe; detaches the daemon from our session.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().map_err(|e| {
        let err = RpcError::new("daemon_unavailable", format!("cannot start engramd: {e}"));
        if e.kind() == io::ErrorKind::PermissionDenied {
            err.hint(
                "Operation not permitted usually means this shell may not gain privileges (an agent or Flatpak sandbox, no_new_privs) \
                 or engramd has the old `=ep` capability: run `engram setup` in a normal terminal, or `sudo setcap cap_sys_ptrace=p $(which engramd)`.",
            )
        } else {
            err
        }
    })?;
    Ok(())
}

fn peer_is_us(s: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: SO_PEERCRED fills a ucred of `len` bytes.
    let r = unsafe { libc::getsockopt(s.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut _, &mut len) };
    r == 0 && cred.uid == unsafe { libc::getuid() }
}
