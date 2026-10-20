//! Value scans: first scan, next scan, results, undo/reset/cancel, and the
//! ranking that puts the most plausible candidates first.

use super::{ensure_idle, invalid, read_value, where_is, R};
use crate::daemon::{core_err, Daemon, Target};
use crate::params::P;
use engram_core::hex;
use engram_core::maps::{self, RegionKind};
use engram_core::scan::{self, Compare, HitRef, PatternFlavor, Query, ScanKind, ScanSession, Scope};
use engram_core::value::{DataType, NumType};
use engram_proto::RpcError;
use serde_json::{json, Value};

pub(super) fn dispatch(d: &Daemon, method: &str, p: &P) -> Option<R> {
    Some(match method {
        "scan" => scan_first(d, p),
        "scan_next" => scan_next(d, p),
        "scan_results" => scan_results(d, p),
        "scan_undo" => {
            let mut st = d.scan.lock().unwrap();
            let Some(prev) = st.undo.take() else {
                return Some(Err(RpcError::new("nothing_to_undo", "no previous scan step")));
            };
            let total = prev.total();
            st.current = Some(prev);
            drop(st);
            d.emit("scan", json!({ "total": total }));
            Ok(json!({ "restored": true, "total": total }))
        }
        "scan_reset" => {
            *d.scan.lock().unwrap() = Default::default();
            d.emit("scan", json!({ "total": 0, "reset": true }));
            Ok(json!({ "reset": true }))
        }
        "cancel" => {
            d.progress.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(json!({ "cancelling": d.busy.lock().unwrap().is_some() }))
        }
        _ => return None,
    })
}

fn kinds_for(type_name: &str, compare: Compare, value: Option<&str>) -> Result<Vec<ScanKind>, RpcError> {
    let pattern = |f| -> Result<Vec<ScanKind>, RpcError> {
        if compare != Compare::Exact {
            return Err(invalid("byte/string searches only support compare=exact"));
        }
        Ok(vec![ScanKind::pattern(f, value.ok_or_else(|| invalid("missing 'value'"))?).map_err(core_err)?])
    };
    match type_name {
        "auto" | "" | "any" => Ok(scan::auto_kinds(compare, value).into_iter().map(ScanKind::Num).collect()),
        "bytes" | "aob" | "pattern" => pattern(PatternFlavor::Bytes),
        "string" | "str" | "text" | "utf8" => pattern(PatternFlavor::Utf8),
        "string16" | "str16" | "utf16" | "wstring" => pattern(PatternFlavor::Utf16),
        other => Ok(vec![ScanKind::Num(NumType::parse_name(other).ok_or_else(|| invalid(format!("unknown scan type '{other}'")))?)]),
    }
}

fn query(p: &P, default: &str) -> Result<Query, RpcError> {
    let compare = Compare::parse(&p.str("compare").unwrap_or_else(|| default.to_string())).map_err(core_err)?;
    let q = Query { compare, value: p.str("value"), value2: p.str("value2") };
    if compare.value_count() >= 1 && q.value.is_none() {
        return Err(invalid(format!("compare={} needs 'value'", compare.name())));
    }
    Ok(q)
}

fn is_system_path(p: &str) -> bool {
    p.starts_with("/usr/") || p.starts_with("/lib") || p.contains("/steamapps/common/Proton") || p.contains("/pressure-vessel/")
}

fn score(kind: &ScanKind, addr: u64, cur: Option<u64>, rk: Option<RegionKind>, system: bool) -> f64 {
    let mut s = match kind {
        ScanKind::Num(NumType::I32) => 3.0,
        ScanKind::Num(NumType::F32) => 2.6,
        ScanKind::Num(NumType::F64) => 1.6,
        ScanKind::Num(NumType::I64) => 1.2,
        _ => 1.0,
    };
    s += match rk {
        Some(RegionKind::Image) if system => 0.5,
        Some(RegionKind::Image) => 2.2,
        Some(RegionKind::Heap) | Some(RegionKind::Anon) => 2.0,
        Some(RegionKind::Stack) => -1.5,
        _ => -3.0,
    };
    if let ScanKind::Num(n) = kind {
        if n.size() == 8 && addr.is_multiple_of(8) {
            s += 0.4;
        }
        if let (true, Some(bits)) = (n.is_float(), cur) {
            let v = n.as_f64(bits);
            if !v.is_finite() {
                s -= 5.0;
            } else if v != 0.0 && !(1e-3..1e7).contains(&v.abs()) {
                s -= 2.0;
            }
        }
    }
    if cur.is_none() {
        s -= 5.0;
    }
    s
}

/// Current raw values for hits (grouped per kind to batch reads).
fn current_bits(t: &Target, s: &ScanSession, hits: &[HitRef]) -> Vec<Option<u64>> {
    let mut out = vec![None; hits.len()];
    for k in 0..s.kinds.len() {
        let idx: Vec<usize> = (0..hits.len()).filter(|i| hits[*i].kind == k).collect();
        if idx.is_empty() {
            continue;
        }
        let size = s.kinds[k].size().min(4096);
        let addrs: Vec<u64> = idx.iter().map(|i| hits[*i].addr).collect();
        let mut buf = vec![0u8; addrs.len() * size];
        let ok = t.mem.read_scatter(&addrs, size, &mut buf);
        for (j, i) in idx.iter().enumerate() {
            if ok[j] {
                let w = &buf[j * size..(j + 1) * size];
                out[*i] = Some(match &s.kinds[k] {
                    ScanKind::Num(n) => n.bits(w),
                    _ => 0,
                });
            }
        }
    }
    out
}

fn fmt_hit(s: &ScanSession, h: &HitRef, cur: Option<u64>, t: &Target, regions: &[maps::Region], mods: &[maps::Module]) -> Value {
    let kind = &s.kinds[h.kind];
    let (value, previous) = match kind {
        ScanKind::Num(n) => (cur.map(|c| n.format(c)), Some(n.format(h.prev))),
        ScanKind::Pattern { flavor, bytes, .. } => {
            let dt = match flavor {
                PatternFlavor::Bytes => DataType::Bytes(bytes.len()),
                PatternFlavor::Utf8 => DataType::Str(bytes.len()),
                PatternFlavor::Utf16 => DataType::Str16(bytes.len() / 2),
            };
            (read_value(t, h.addr, dt), None)
        }
    };
    let mut v = json!({ "address": hex(h.addr), "type": kind.name(), "value": value, "where": where_is(regions, mods, h.addr) });
    if let (Some(prev), Some(cur)) = (previous, &value) {
        if &prev != cur {
            v["previous"] = json!(prev);
        }
    }
    v
}

fn ranked(t: &Target, s: &ScanSession, limit: usize) -> Vec<Value> {
    let (regions, mods) = t.layout();
    let pool: Vec<HitRef> =
        if s.total() <= 20_000 { s.hits(None, 0, 20_000) } else { (0..s.kinds.len()).flat_map(|k| s.hits(Some(k), 0, limit)).collect() };
    let cur = current_bits(t, s, &pool);
    let mut scored: Vec<(f64, usize)> = pool
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let r = maps::region_for(&regions, h.addr);
            let system = maps::module_for(&mods, h.addr).is_some_and(|m| is_system_path(&m.path));
            (score(&s.kinds[h.kind], h.addr, cur[i], r.map(|r| r.kind()), system), i)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(pool[a.1].addr.cmp(&pool[b.1].addr)));
    let mut seen = std::collections::HashSet::new();
    scored
        .into_iter()
        .filter(|(_, i)| seen.insert(pool[*i].addr))
        .take(limit)
        .map(|(_, i)| fmt_hit(s, &pool[i], cur[i], t, &regions, &mods))
        .collect()
}

fn scan_summary(t: &Target, s: &ScanSession, limit: usize) -> Value {
    let total = s.total();
    let by_type: serde_json::Map<String, Value> = s.kinds.iter().zip(&s.counts).map(|(k, c)| (k.name(), json!(c))).collect();
    let hint = if total == 0 {
        "No matches. Likely causes: different type (try type=auto or f32/f64), the shown number is scaled (percent, x10), \
         or the value changed mid-scan. Start over with scan (compare=unknown if unsure)."
            .to_string()
    } else if total as usize > limit {
        format!("{total} candidates. Ask the user to change the value in-game, then scan_next compare=exact value=<new> (or changed/increased/decreased).")
    } else {
        "Few candidates left. Verify with analyze or sample (or a test write the user confirms), then table_add.".to_string()
    };
    json!({
        "step": s.step,
        "compare": s.last_compare.name(),
        "total": total,
        "by_type": by_type,
        "scanned_mb": s.scanned_bytes / (1 << 20),
        "memory_mb": s.memory_bytes() / (1 << 20),
        "candidates": ranked(t, s, limit),
        "hint": hint,
    })
}

fn scan_first(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    ensure_idle(d)?;
    let q = query(p, "exact")?;
    let type_name = p.str("type").unwrap_or_else(|| "auto".into()).to_lowercase();
    let kinds = kinds_for(&type_name, q.compare, q.value.as_deref())?;
    let aligned = p.bool("aligned").unwrap_or(true);
    let mut scope = Scope { writable_only: p.bool("writable_only").unwrap_or(true), ..Default::default() };
    let (regions, mods) = t.fresh_layout();
    if let Some(m) = p.str("module") {
        let m = maps::find_module(&mods, &m).ok_or_else(|| RpcError::new("unknown_module", format!("no module '{m}'")))?;
        scope.start = m.base;
        scope.end = m.end;
    }
    let limit = p.int_or("limit", 15, 0, 200) as usize;
    *d.scan.lock().unwrap() = Default::default(); // free the old session before allocating a new one
    let s = d.long_op("scan", || ScanSession::first(&t.mem, &regions, kinds, aligned, &q, &scope, &d.progress).map_err(core_err))?;
    let summary = scan_summary(&t, &s, limit);
    let total = s.total();
    d.scan.lock().unwrap().current = Some(s);
    d.emit("scan", json!({ "total": total }));
    Ok(summary)
}

fn scan_next(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    ensure_idle(d)?;
    let q = query(p, "exact")?;
    let limit = p.int_or("limit", 15, 0, 200) as usize;
    let old =
        d.scan.lock().unwrap().current.take().ok_or_else(|| RpcError::new("no_scan", "no scan in progress").hint("start with scan"))?;
    match d.long_op("scan", || old.next(&t.mem, &q, &d.progress).map_err(core_err)) {
        Ok(new) => {
            let summary = scan_summary(&t, &new, limit);
            let total = new.total();
            let mut st = d.scan.lock().unwrap();
            st.undo = Some(old);
            st.current = Some(new);
            drop(st);
            d.emit("scan", json!({ "total": total }));
            Ok(summary)
        }
        Err(e) => {
            d.scan.lock().unwrap().current = Some(old);
            Err(e)
        }
    }
}

fn scan_results(d: &Daemon, p: &P) -> R {
    let t = d.target()?;
    let st = d.scan.lock().unwrap();
    let s = match st.current.as_ref() {
        Some(s) => s,
        None if d.busy.lock().unwrap().is_some() => return Err(RpcError::new("busy", "a scan is running")),
        None => return Err(RpcError::new("no_scan", "no scan in progress").hint("start with scan")),
    };
    let kind = match p.str("type") {
        Some(name) => Some(s.kinds.iter().position(|k| k.name() == name).ok_or_else(|| invalid(format!("scan has no type '{name}'")))?),
        None => None,
    };
    let offset = p.int_or("offset", 0, 0, i64::MAX) as u64;
    let limit = p.int_or("limit", 50, 1, 1000) as usize;
    let hits = s.hits(kind, offset, limit);
    let cur = current_bits(&t, s, &hits);
    let (regions, mods) = t.layout();
    let list: Vec<Value> = hits.iter().zip(&cur).map(|(h, c)| fmt_hit(s, h, *c, &t, &regions, &mods)).collect();
    let total = kind.map_or(s.total(), |k| s.counts[k]);
    Ok(json!({ "total": total, "offset": offset, "results": list }))
}
