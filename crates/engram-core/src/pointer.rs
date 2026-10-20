//! Pointer-path scanner: find `[[module+off]+o1]+o2` chains that lead to an
//! address, so a found value can be re-located after the game restarts.

use crate::maps::{module_for, Module, Region};
use crate::memory::ProcMem;
use crate::scan::{plan, Progress, Scope};
use crate::{Error, Result};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::atomic::Ordering;

#[derive(Debug, Clone)]
pub struct Options {
    pub max_depth: usize,
    pub max_offset: u64,
    pub max_results: usize,
    pub ptr_size: usize,
    /// Prefer paths rooted in this module (the game executable).
    pub prefer_module: Option<String>,
    /// Hard cap on collected pointers to bound memory use (16 bytes each).
    pub max_pointers: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { max_depth: 3, max_offset: 0x1000, max_results: 20, ptr_size: 8, prefer_module: None, max_pointers: 80_000_000 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PointerPath {
    pub expr: String,
    pub module: String,
    pub depth: usize,
    #[serde(skip)]
    pub base_offset: u64,
    #[serde(skip)]
    pub offsets: Vec<u64>,
}

pub fn build_expr(module: &str, base_offset: u64, offsets: &[u64]) -> String {
    let m = if module.contains([' ', '-', '+', '[', ']']) { format!("\"{module}\"") } else { module.to_string() };
    let mut e = format!("{m}+0x{base_offset:x}");
    for o in offsets {
        e = format!("[{e}]+0x{o:x}");
    }
    e
}

pub fn scan(
    mem: &ProcMem,
    regions: &[Region],
    mods: &[Module],
    target: u64,
    opt: &Options,
    progress: &Progress,
) -> Result<Vec<PointerPath>> {
    let ps = opt.ptr_size;
    let readable: Vec<(u64, u64)> = regions.iter().filter(|r| r.scannable()).map(|r| (r.start, r.end)).collect();
    let (lo_all, hi_all) = (readable.first().map_or(0, |r| r.0), readable.last().map_or(0, |r| r.1));
    let valid = |v: u64| -> bool {
        if v < lo_all || v >= hi_all {
            return false;
        }
        let i = readable.partition_point(|r| r.1 <= v);
        readable.get(i).is_some_and(|r| v >= r.0)
    };

    // 1. Collect (value, location) for every aligned pointer-looking value in writable memory.
    let specs = plan(regions, &Scope::default());
    progress.reset(specs.iter().map(|s| s.1 as u64).sum());
    let mut ptrs: Vec<(u64, u64)> = specs
        .par_iter()
        .flat_map_iter(|&(base, len)| {
            let mut buf = vec![0u8; len];
            let n = mem.read(base, &mut buf).unwrap_or(0);
            progress.done.fetch_add(len as u64, Ordering::Relaxed);
            let mut local = Vec::new();
            let mut off = (ps - (base as usize % ps)) % ps;
            while off + ps <= n {
                let v = if ps == 4 {
                    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as u64
                } else {
                    u64::from_le_bytes(buf[off..off + 8].try_into().unwrap())
                };
                if valid(v) {
                    local.push((v, base + off as u64));
                }
                off += ps;
            }
            local.into_iter()
        })
        .collect();
    if progress.cancel.load(Ordering::Relaxed) {
        return Err(Error::new("cancelled", "pointer scan cancelled"));
    }
    if ptrs.len() > opt.max_pointers {
        return Err(Error::new(
            "too_large",
            format!("{} pointers exceed the {} cap; narrow the target first", ptrs.len(), opt.max_pointers),
        ));
    }
    ptrs.par_sort_unstable();

    // 2. Walk backwards from the target, level by level.
    let static_mod = |loc: u64| module_for(mods, loc);
    let mut results: Vec<PointerPath> = Vec::new();
    let mut frontier: Vec<(u64, Vec<u64>)> = vec![(target, Vec::new())];
    let mut seen: HashSet<u64> = HashSet::new();
    const MAX_FRONTIER: usize = 50_000;
    for depth in 1..=opt.max_depth {
        let mut next = Vec::new();
        for (addr, offs) in &frontier {
            let lo = addr.saturating_sub(opt.max_offset);
            let start = ptrs.partition_point(|p| p.0 < lo);
            for &(v, loc) in ptrs[start..].iter().take_while(|p| p.0 <= *addr) {
                let mut path = Vec::with_capacity(offs.len() + 1);
                path.push(addr - v);
                path.extend_from_slice(offs);
                if let Some(m) = static_mod(loc) {
                    results.push(PointerPath {
                        expr: build_expr(&m.name, loc - m.base, &path),
                        module: m.name.clone(),
                        depth,
                        base_offset: loc - m.base,
                        offsets: path,
                    });
                } else if depth < opt.max_depth && next.len() < MAX_FRONTIER && seen.insert(loc) {
                    next.push((loc, path));
                }
            }
        }
        if results.len() >= opt.max_results * 8 || next.is_empty() {
            break;
        }
        // Explore the most promising (smallest total offset) chains first.
        next.sort_by_key(|(_, p)| p.iter().sum::<u64>());
        frontier = next;
    }

    let prefer = opt.prefer_module.as_deref().unwrap_or("");
    let rank = |p: &PointerPath| {
        let main = !p.module.eq_ignore_ascii_case(prefer);
        let system = p.module.starts_with("lib") || p.module.ends_with(".so") || p.module.contains(".so.");
        (p.depth, main, system, p.offsets.iter().sum::<u64>())
    };
    results.sort_by_key(rank);
    results.truncate(opt.max_results);
    Ok(results)
}
