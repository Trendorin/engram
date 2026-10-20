//! Messages to the user and engine control: notify, approvals, policy,
//! activity log, settings reload, version handshake and shutdown. Apart from
//! `notify` these are internal methods (`engram_proto::tools::INTERNAL_METHODS`).

use super::{invalid, R};
use crate::daemon::{Daemon, Decision, Policy, GUI};
use crate::params::P;
use engram_proto::{paths, RpcError};
use serde_json::json;
use std::time::Duration;

pub(super) fn dispatch(d: &Daemon, client: &str, method: &str, p: &P) -> Option<R> {
    Some(match method {
        "notify" => notify(d, client, p),
        "hello" => Ok(json!({ "engram": engram_proto::VERSION, "protocol": engram_proto::PROTOCOL, "access": d.access_mode() })),
        "policy" => policy(d, client, p),
        "approval_respond" => approval_respond(d, client, p),
        "activity" => Ok(json!({ "activity": d.recent_activity(p.int_or("limit", 100, 1, 300) as usize) })),
        "reload_config" => {
            d.reload_config();
            Ok(json!({ "reloaded": true }))
        }
        "shutdown" => {
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(100));
                let _ = std::fs::remove_file(paths::socket());
                std::process::exit(0);
            });
            Ok(json!({ "shutting_down": true }))
        }
        _ => return None,
    })
}

fn notify(d: &Daemon, client: &str, p: &P) -> R {
    let message = p.req_str("message")?;
    let level = p.str("level").unwrap_or_else(|| "info".into());
    d.emit("notify", json!({ "client": client, "message": message, "level": level }));
    d.log_activity(client, "notify", message, true);
    Ok(json!({ "shown": true }))
}

fn policy(d: &Daemon, client: &str, p: &P) -> R {
    if let Some(mode) = p.str("mode") {
        if client != GUI {
            return Err(RpcError::new("forbidden", "only the Engram window can change the approval policy"));
        }
        let pol = Policy::parse(&mode).ok_or_else(|| invalid("mode must be ask|allow|deny"))?;
        *d.policy.lock().unwrap() = pol;
        if pol != Policy::Allow {
            d.revoke_sessions();
        }
        d.emit("policy", json!({ "policy": pol.name() }));
    }
    Ok(json!({ "policy": d.policy.lock().unwrap().name(), "pending": d.pending_approvals() }))
}

fn approval_respond(d: &Daemon, client: &str, p: &P) -> R {
    if client != GUI {
        return Err(RpcError::new("forbidden", "only the Engram window can answer approvals"));
    }
    let id = p.int("id").ok_or_else(|| invalid("missing 'id'"))? as u64;
    let dec = match p.req_str("decision")?.as_str() {
        "allow" => Decision::Allow,
        "allow_session" => Decision::AllowSession,
        "deny" => Decision::Deny,
        other => return Err(invalid(format!("unknown decision '{other}'"))),
    };
    Ok(json!({ "accepted": d.respond(id, dec) }))
}
