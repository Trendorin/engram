//! Tool implementations, one module per group of the catalog in
//! `engram_proto::tools`. `handle` logs agent activity and routes a method to
//! the first module that knows it; each module's `dispatch` returns `None` for
//! methods it does not own.

mod analysis;
mod control;
mod memory;
mod scan;
mod session;
mod table;

use crate::daemon::{core_err, Daemon, Target, GUI};
use crate::params::P;
use engram_core::maps::{self, RegionKind};
use engram_core::value::DataType;
use engram_proto::tools::{self, Access};
use engram_proto::RpcError;
use serde_json::Value;

pub(crate) type R = Result<Value, RpcError>;

pub(crate) fn invalid(msg: impl Into<String>) -> RpcError {
    RpcError::new("invalid_argument", msg)
}

pub fn handle(d: &Daemon, client: &str, method: &str, params: &Value) -> R {
    let method = method.strip_prefix("engram_").unwrap_or(method);
    let r = dispatch(d, client, method, &P(params));
    let access = tools::find(method).map(|t| t.access);
    // Everything an agent does is shown; the GUI's own polling is not.
    let log = client != GUI || access == Some(Access::Mutate);
    // `notify` logs itself; internal plumbing (launch_status polling, hello, ...) is noise.
    if log && method != "notify" && !tools::INTERNAL_METHODS.contains(&method) {
        let mut args = params.to_string();
        if args == "null" || args == "{}" {
            args.clear();
        }
        if args.len() > 160 {
            let cut = (0..=160).rev().find(|i| args.is_char_boundary(*i)).unwrap_or(0);
            args.truncate(cut);
            args.push('…');
        }
        let summary = match &r {
            Ok(_) => args,
            Err(e) => format!("{args} → {}", e.code),
        };
        d.log_activity(client, method, summary, r.is_ok());
    }
    r
}

fn dispatch(d: &Daemon, client: &str, method: &str, p: &P) -> R {
    let routed = session::dispatch(d, client, method, p)
        .or_else(|| scan::dispatch(d, method, p))
        .or_else(|| table::dispatch(d, client, method, p))
        .or_else(|| memory::dispatch(d, client, method, p))
        .or_else(|| analysis::dispatch(d, client, method, p))
        .or_else(|| control::dispatch(d, client, method, p));
    routed
        .unwrap_or_else(|| Err(RpcError::new("unknown_tool", format!("no tool named '{method}'")).hint("run `engram tools` for the list")))
}

// ---- helpers shared by the tool modules ------------------------------------------

pub(crate) fn dtype(p: &P, key: &str, default: &str) -> Result<DataType, RpcError> {
    p.str(key).unwrap_or_else(|| default.to_string()).parse::<DataType>().map_err(core_err)
}

pub(crate) fn read_value(t: &Target, addr: u64, dt: DataType) -> Option<String> {
    let mut b = vec![0u8; dt.size(t.ptr_size)];
    t.mem.read_exact(addr, &mut b).ok()?;
    Some(dt.format(&b))
}

pub(crate) fn where_is(regions: &[maps::Region], mods: &[maps::Module], addr: u64) -> String {
    if maps::module_for(mods, addr).is_some() {
        return maps::symbolize(mods, addr);
    }
    match maps::region_for(regions, addr).map(|r| r.kind()) {
        Some(RegionKind::Heap) => "heap".into(),
        Some(RegionKind::Stack) => "stack".into(),
        Some(RegionKind::Anon) => "anon".into(),
        Some(RegionKind::Image) => "mapped file".into(),
        Some(RegionKind::Special) => "special".into(),
        None => "unmapped".into(),
    }
}

pub(crate) fn ensure_idle(d: &Daemon) -> Result<(), RpcError> {
    match *d.busy.lock().unwrap() {
        Some(op) => Err(RpcError::new("busy", format!("a {op} is running")).hint("wait for it, or call cancel")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Policy;
    use serde_json::json;

    /// Every catalog tool and internal method must reach a handler. With no
    /// target attached and no arguments none of them has side effects.
    #[test]
    fn every_tool_is_routed() {
        let d = Daemon::new(Policy::Deny, Default::default());
        let names = tools::catalog().iter().map(|t| t.name).chain(tools::INTERNAL_METHODS.iter().copied());
        for name in names.filter(|n| !matches!(*n, "shutdown" | "subscribe")) {
            if let Err(e) = dispatch(&d, "test", name, &P(&json!({}))) {
                assert_ne!(e.code, "unknown_tool", "{name} has no handler");
            }
        }
    }
}
