//! The address table: list, add, edit (value/freeze need approval for agents), remove.

use super::{dtype, invalid, read_value, R};
use crate::daemon::{core_err, Daemon};
use crate::params::P;
use engram_core::hex;
use engram_proto::RpcError;
use serde_json::{json, Value};

pub(super) fn dispatch(d: &Daemon, client: &str, method: &str, p: &P) -> Option<R> {
    Some(match method {
        "table_list" => table_list(d),
        "table_add" => table_add(d, p),
        "table_set" => table_set(d, client, p),
        "table_remove" => table_remove(d, p),
        _ => return None,
    })
}

fn table_list(d: &Daemon) -> R {
    let t = d.target.read().unwrap().clone();
    let tab = d.table.lock().unwrap();
    let entries: Vec<Value> = tab
        .entries
        .iter()
        .map(|e| {
            let addr = t.as_ref().and_then(|t| t.resolve(&e.address).ok());
            let value = match (&t, addr) {
                (Some(t), Some(a)) => read_value(t, a, e.dtype),
                _ => None,
            };
            json!({
                "id": e.id, "description": e.description, "address": e.address,
                "resolved": addr.map(hex), "type": e.dtype, "value": value, "frozen": e.frozen.is_some(),
            })
        })
        .collect();
    Ok(json!({ "game": tab.game, "entries": entries }))
}

fn table_add(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let expr = p.req_str("address")?;
    let addr = t.resolve(&expr)?;
    let dt = dtype(p, "type", "i32")?;
    let value = read_value(&t, addr, dt);
    let mut tab = d.table.lock().unwrap();
    let description = p.str("description").unwrap_or_else(|| format!("Entry {}", tab.entries.len() + 1));
    let id = tab.add(description, expr.trim().to_string(), dt);
    drop(tab);
    d.emit("table", json!({}));
    Ok(json!({ "id": id, "resolved": hex(addr), "value": value }))
}

fn table_set(d: &Daemon, client: &str, p: &P) -> R {
    let id = p.int("id").ok_or_else(|| invalid("missing 'id'"))? as u32;
    let mut entry = d
        .table
        .lock()
        .unwrap()
        .entries
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or_else(|| RpcError::new("not_found", format!("no entry {id}")))?;
    let t = d.target.read().unwrap().clone();
    if let Some(desc) = p.str("description") {
        entry.description = desc;
    }
    let (old_address, old_dtype) = (entry.address.clone(), entry.dtype);
    if let Some(a) = p.str("address") {
        if let Some(t) = &t {
            t.resolve(&a)?;
        }
        entry.address = a.trim().to_string();
    }
    if p.str("type").is_some() {
        entry.dtype = dtype(p, "type", "i32")?;
    }
    // A freeze holds bytes for one address and type. Moving it would write the old bytes somewhere
    // new without the approval a write needs, so the freeze ends; freezing again asks.
    if entry.frozen.is_some() && (entry.address != old_address || entry.dtype != old_dtype) {
        entry.frozen = None;
    }
    let value = p.str("value");
    let freeze = p.bool("freeze");
    let mut written = None;
    if value.is_some() || freeze == Some(true) {
        let t = d.target()?;
        let addr = t.resolve(&entry.address)?;
        let cur = read_value(&t, addr, entry.dtype).unwrap_or_else(|| "?".into());
        let reason = p.str("reason").map(|r| format!("{r} — ")).unwrap_or_default();
        let what = match (&value, freeze) {
            (Some(v), Some(true)) => format!("set and freeze '{}' at {v} (currently {cur})", entry.description),
            (Some(v), _) => format!("set '{}' to {v} (currently {cur})", entry.description),
            (None, _) => format!("freeze '{}' at {cur}", entry.description),
        };
        d.approve(client, "table_set", &format!("{reason}{what}"))?;
        let bytes = match &value {
            Some(v) => entry.dtype.encode(v, t.ptr_size).map_err(core_err)?,
            None => t.mem.read_vec(addr, entry.dtype.size(t.ptr_size)).map_err(core_err)?,
        };
        if value.is_some() {
            t.mem.write(addr, &bytes).map_err(core_err)?;
            written = read_value(&t, addr, entry.dtype);
        }
        // Editing the value of a frozen entry changes what is held; otherwise the freezer would
        // write the old bytes back a moment later.
        if freeze == Some(true) || (value.is_some() && entry.frozen.is_some() && freeze != Some(false)) {
            entry.frozen = Some(bytes);
        }
    }
    if freeze == Some(false) {
        entry.frozen = None;
    }
    let mut tab = d.table.lock().unwrap();
    let e = tab.get_mut(id).ok_or_else(|| RpcError::new("not_found", format!("entry {id} was removed")))?;
    *e = entry.clone();
    tab.save();
    drop(tab);
    d.emit("table", json!({}));
    Ok(
        json!({ "id": id, "description": entry.description, "address": entry.address, "type": entry.dtype, "frozen": entry.frozen.is_some(), "value": written }),
    )
}

fn table_remove(d: &Daemon, p: &P) -> R {
    let id = p.int("id").ok_or_else(|| invalid("missing 'id'"))? as u32;
    let mut tab = d.table.lock().unwrap();
    let before = tab.entries.len();
    tab.entries.retain(|e| e.id != id);
    if tab.entries.len() == before {
        return Err(RpcError::new("not_found", format!("no entry {id}")));
    }
    tab.save();
    drop(tab);
    d.emit("table", json!({}));
    Ok(json!({ "removed": id }))
}
