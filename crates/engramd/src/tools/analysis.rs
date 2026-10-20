//! Understanding addresses, structures and code: analyze, dissect,
//! disassemble, pointer scan, hardware watchpoints and sampling.

use super::{dtype, ensure_idle, invalid, read_value, R};
use crate::daemon::{core_err, Daemon};
use crate::params::P;
use engram_core::{analyze, disasm, hex, maps, pointer, watch};
use serde_json::{json, Value};
use std::time::Duration;

pub(super) fn dispatch(d: &Daemon, client: &str, method: &str, p: &P) -> Option<R> {
    Some(match method {
        "analyze" => analyze_cmd(d, p),
        "dissect" => dissect(d, p),
        "disassemble" => disassemble(d, p),
        "pointer_scan" => pointer_scan(d, p),
        "watch" => watch_cmd(d, client, p),
        "sample" => sample(d, p),
        _ => return None,
    })
}

fn analyze_cmd(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let addr = t.resolve(&p.req_str("address")?)?;
    let (regions, mods) = t.layout();
    let info = analyze::describe(&t.mem, &regions, &mods, addr, t.ptr_size).map_err(core_err)?;
    Ok(serde_json::to_value(info).unwrap_or(Value::Null))
}

fn dissect(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let addr = t.resolve(&p.req_str("address")?)?;
    let size = p.int_or("size", 128, 8, 4096) as usize;
    let (regions, mods) = t.layout();
    let fields = analyze::dissect(&t.mem, &regions, &mods, addr, size, t.ptr_size).map_err(core_err)?;
    Ok(json!({ "base": hex(addr), "location": maps::symbolize(&mods, addr), "fields": fields }))
}

fn disassemble(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let addr = t.resolve(&p.req_str("address")?)?;
    let count = p.int_or("count", 16, 1, 200) as usize;
    let (_, mods) = t.layout();
    let ins = disasm::disassemble(&t.mem, addr, count, t.bitness(), &mods).map_err(core_err)?;
    Ok(json!({ "instructions": ins }))
}

fn pointer_scan(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    ensure_idle(d)?;
    let addr = t.resolve(&p.req_str("address")?)?;
    let max_offset = p.str("max_offset").and_then(|s| engram_core::parse_addr(&s)).unwrap_or(0x1000).clamp(8, 0x10000);
    let opt = pointer::Options {
        max_depth: p.int_or("max_depth", 3, 1, 5) as usize,
        max_offset,
        max_results: p.int_or("limit", 10, 1, 200) as usize,
        ptr_size: t.ptr_size,
        prefer_module: Some(t.name.clone()),
        ..Default::default()
    };
    let (regions, mods) = t.fresh_layout();
    let paths = d.long_op("pointer_scan", || pointer::scan(&t.mem, &regions, &mods, addr, &opt, &d.progress).map_err(core_err))?;
    let list: Vec<Value> = paths
        .iter()
        .map(|pp| json!({ "expr": pp.expr, "module": pp.module, "depth": pp.depth, "verified": t.resolve(&pp.expr).ok() == Some(addr) }))
        .collect();
    let hint = if list.is_empty() {
        "No static path found. Try max_depth=4 or max_offset=0x2000, or use watch to find the code and its base register."
    } else {
        "Store the first verified path with table_set id=<id> address=<expr>. Re-check it after restarting the game; keep the one that survives."
    };
    Ok(json!({ "target": hex(addr), "count": list.len(), "paths": list, "hint": hint }))
}

fn watch_cmd(d: &Daemon, client: &str, p: &P) -> R {
    let t = d.target()?;
    ensure_idle(d)?;
    let expr = p.req_str("address")?;
    let addr = t.resolve(&expr)?;
    let size = p.int_or("size", 4, 1, 8) as usize;
    let kind = match p.str("kind").as_deref().unwrap_or("write") {
        "write" | "writes" => watch::WatchKind::Write,
        "access" | "read" | "rw" => watch::WatchKind::Access,
        other => return Err(invalid(format!("kind must be write|access, not '{other}'"))),
    };
    let ms = p.int_or("duration_ms", 5000, 100, 60_000) as u64;
    let max_hits = p.int_or("max_hits", 2000, 1, 1_000_000) as u64;
    d.approve(
        client,
        "watch",
        &format!(
            "watch {} {} for {:.1}s (traces the game's threads briefly)",
            if kind == watch::WatchKind::Write { "writes to" } else { "accesses to" },
            expr,
            ms as f64 / 1000.0
        ),
    )?;
    d.progress.reset(0);
    let report = d.long_op("watch", || {
        watch::watch(t.pid, addr, size, kind, Duration::from_millis(ms), max_hits, &d.progress.cancel).map_err(core_err)
    })?;
    let (_, mods) = t.fresh_layout();
    let mut hits: Vec<_> = report.hits.iter().collect();
    hits.sort_by_key(|h| std::cmp::Reverse(h.count));
    let list: Vec<Value> = hits
        .iter()
        .take(30)
        .map(|h| {
            let ins = disasm::instruction_before(&t.mem, h.rip_after, t.bitness(), &mods);
            json!({
                "address": ins.as_ref().map(|i| i.address.clone()).unwrap_or_else(|| hex(h.rip_after)),
                "location": ins.as_ref().map(|i| i.location.clone()).unwrap_or_else(|| maps::symbolize(&mods, h.rip_after)),
                "instruction": ins.as_ref().map(|i| i.text.clone()),
                "count": h.count,
                "registers": h.first_regs,
            })
        })
        .collect();
    let hint = if list.is_empty() {
        "Nothing touched the address during the window. Make sure the user triggered the change while watching, or try kind=access."
    } else {
        "The instruction's memory operand shows the struct base register (e.g. [rcx+0x18] → object at rcx). disassemble around it for context."
    };
    Ok(json!({ "address": hex(addr), "threads": report.threads, "elapsed_ms": report.elapsed_ms, "instructions": list, "hint": hint }))
}

fn sample(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let addr = t.resolve(&p.req_str("address")?)?;
    let dt = dtype(p, "type", "i32")?;
    let n = p.int_or("samples", 10, 1, 200) as usize;
    let interval = p.int_or("interval_ms", 250, 10, 2000) as u64;
    let n = n.min((60_000 / interval.max(1)) as usize).max(1);
    let mut series = Vec::with_capacity(n);
    for i in 0..n {
        series.push(read_value(&t, addr, dt));
        if i + 1 < n {
            std::thread::sleep(Duration::from_millis(interval));
        }
    }
    let changes = series.windows(2).filter(|w| w[0] != w[1]).count();
    let mut distinct: Vec<&Option<String>> = series.iter().collect();
    distinct.sort();
    distinct.dedup();
    let nums: Vec<f64> = series.iter().filter_map(|v| v.as_ref()?.parse().ok()).collect();
    let (min, max) = nums.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    Ok(json!({
        "address": hex(addr), "type": dt, "interval_ms": interval, "values": series,
        "changes": changes, "distinct": distinct.len(),
        "min": nums.first().map(|_| min), "max": nums.first().map(|_| max),
    }))
}
