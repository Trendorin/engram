//! Memory layout, reading and writing.

use super::{dtype, invalid, R};
use crate::daemon::{core_err, Daemon};
use crate::params::P;
use engram_core::hex;
use engram_core::maps;
use engram_core::value::DataType;
use engram_proto::RpcError;
use serde_json::{json, Value};

pub(super) fn dispatch(d: &Daemon, client: &str, method: &str, p: &P) -> Option<R> {
    Some(match method {
        "modules" => modules(d, p),
        "regions" => regions(d, p),
        "read" => read(d, p),
        "hexdump" => hexdump(d, p),
        "write" => write(d, client, p),
        _ => return None,
    })
}

fn rounding_half(text: &str) -> f64 {
    let decimals = text.trim().split_once('.').map(|(_, d)| d.len()).unwrap_or(0) as i32;
    0.5 * 10f64.powi(-decimals)
}

fn matches_expected(dt: DataType, cur: &[u8], expected: &str, ptr_size: usize) -> bool {
    match dt {
        DataType::Num(n) if n.is_float() => match expected.trim().parse::<f64>() {
            Ok(v) => (n.as_f64(n.bits(cur)) - v).abs() <= rounding_half(expected).max(v.abs() * 1e-6),
            Err(_) => false,
        },
        DataType::Str(_) | DataType::Str16(_) => dt.format(cur).starts_with(expected),
        _ => dt.encode(expected, ptr_size).map(|e| cur.starts_with(&e)).unwrap_or(false),
    }
}

fn modules(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let (_, mods) = t.fresh_layout();
    let q = p.str("query").map(|s| s.to_lowercase());
    let limit = p.int_or("limit", 40, 1, 2000) as usize;
    let list: Vec<Value> = mods
        .iter()
        .filter(|m| q.as_ref().is_none_or(|q| m.name.to_lowercase().contains(q) || m.path.to_lowercase().contains(q)))
        .map(|m| json!({ "name": m.name, "base": hex(m.base), "size": hex(m.end - m.base), "path": m.path }))
        .collect();
    Ok(json!({ "total": list.len(), "modules": list.into_iter().take(limit).collect::<Vec<_>>() }))
}

fn regions(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let (regions, mods) = t.fresh_layout();
    let wo = p.bool("writable_only").unwrap_or(false);
    let kind = p.str("kind");
    let limit = p.int_or("limit", 50, 1, 5000) as usize;
    let sel: Vec<_> = regions
        .iter()
        .filter(|r| !wo || r.writable())
        .filter(|r| {
            kind.as_ref().is_none_or(|k| serde_json::to_value(r.kind()).ok().and_then(|v| v.as_str().map(|s| s == k)).unwrap_or(false))
        })
        .collect();
    let total_mb: u64 = sel.iter().map(|r| r.size()).sum::<u64>() / (1 << 20);
    let list: Vec<Value> = sel
        .iter()
        .take(limit)
        .map(|r| {
            json!({
                "start": hex(r.start), "end": hex(r.end), "size_kb": r.size() / 1024, "perms": r.perms,
                "kind": r.kind(), "location": maps::module_for(&mods, r.start).map(|m| m.name.clone()).unwrap_or_else(|| r.path.clone()),
            })
        })
        .collect();
    Ok(json!({ "total": sel.len(), "total_mb": total_mb, "regions": list }))
}

fn read(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let addr = t.resolve(&p.req_str("address")?)?;
    let dt = dtype(p, "type", "i32")?;
    let count = p.int_or("count", 1, 1, 256) as usize;
    let size = dt.size(t.ptr_size);
    let mut buf = vec![0u8; size * count];
    t.mem.read_exact(addr, &mut buf).map_err(core_err)?;
    let values: Vec<String> = buf.chunks(size).map(|c| dt.format(c)).collect();
    if count == 1 {
        Ok(json!({ "address": hex(addr), "type": dt, "value": values[0] }))
    } else {
        Ok(json!({ "address": hex(addr), "type": dt, "values": values }))
    }
}

fn hexdump(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let addr = t.resolve(&p.req_str("address")?)?;
    let size = p.int_or("size", 256, 1, 4096) as usize;
    let mut buf = vec![0u8; size];
    let n = t.mem.read(addr, &mut buf).map_err(|e| core_err(e.into()))?;
    if n == 0 {
        return Err(RpcError::new("unreadable", format!("{} is not readable", hex(addr))));
    }
    let lines: Vec<String> = buf[..n]
        .chunks(16)
        .enumerate()
        .map(|(i, c)| {
            let hexs: Vec<String> = c.iter().map(|b| format!("{b:02x}")).collect();
            let ascii: String = c.iter().map(|b| if b.is_ascii_graphic() || *b == b' ' { *b as char } else { '.' }).collect();
            format!("{:x}  {:<47}  {}", addr + (i * 16) as u64, hexs.join(" "), ascii)
        })
        .collect();
    Ok(json!({ "address": hex(addr), "bytes": n, "dump": lines }))
}

fn write(d: &Daemon, client: &str, p: &P) -> R {
    let t = d.target()?;
    let expr = p.req_str("address")?;
    let addr = t.resolve(&expr)?;
    let dt = dtype(p, "type", "i32")?;
    let value = p.str("value").ok_or_else(|| invalid("missing 'value'"))?;
    let bytes = dt.encode(&value, t.ptr_size).map_err(core_err)?;
    let size = match dt {
        DataType::Num(_) | DataType::Ptr => dt.size(t.ptr_size),
        _ => bytes.len(),
    };
    let mut old = vec![0u8; size.max(1)];
    t.mem.read_exact(addr, &mut old).map_err(core_err)?;
    let old_txt = dt.format(&old);
    if let Some(exp) = p.str("expected") {
        if !matches_expected(dt, &old, &exp, t.ptr_size) {
            return Err(RpcError::new("stale_value", format!("memory at {} holds {old_txt}, not {exp}", hex(addr)))
                .hint("re-read the value (it may have changed in-game) and retry with the new expected value"));
        }
    }
    let reason = p.str("reason").map(|r| format!("{r} — ")).unwrap_or_default();
    let where_ = if expr.trim() == hex(addr) { hex(addr) } else { format!("{expr} ({})", hex(addr)) };
    d.approve(client, "write", &format!("{reason}write {value} ({dt}) to {where_}; currently {old_txt}"))?;
    t.mem.write(addr, &bytes).map_err(core_err)?;
    let mut now = vec![0u8; size.max(1)];
    let new_txt = t.mem.read_exact(addr, &mut now).map(|_| dt.format(&now)).unwrap_or_default();
    d.emit("memory", json!({ "address": hex(addr) }));
    Ok(json!({ "address": hex(addr), "type": dt, "old": old_txt, "new": new_txt, "verified": now == bytes }))
}
