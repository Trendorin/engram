//! `/proc/<pid>/maps` parsing, region classification and module discovery.

use serde::Serialize;
use std::fs;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RegionKind {
    Heap,
    Stack,
    Anon,
    Image,
    Special,
}

#[derive(Debug, Clone, Serialize)]
pub struct Region {
    pub start: u64,
    pub end: u64,
    pub perms: String,
    pub offset: u64,
    pub inode: u64,
    pub path: String,
}

impl Region {
    fn perm(&self, i: usize) -> bool {
        self.perms.as_bytes().get(i).is_some_and(|c| *c != b'-')
    }
    pub fn readable(&self) -> bool {
        self.perm(0)
    }
    pub fn writable(&self) -> bool {
        self.perm(1)
    }
    pub fn executable(&self) -> bool {
        self.perm(2)
    }
    pub fn size(&self) -> u64 {
        self.end - self.start
    }
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end
    }
    pub fn kind(&self) -> RegionKind {
        let p = self.path.as_str();
        if p.is_empty() || p.starts_with("[anon") || p.starts_with("/memfd:") {
            RegionKind::Anon
        } else if p == "[heap]" {
            RegionKind::Heap
        } else if p.starts_with("[stack") {
            RegionKind::Stack
        } else if p.starts_with('[') || p.starts_with("/dev/") || p.starts_with("/SYSV") {
            RegionKind::Special
        } else {
            RegionKind::Image
        }
    }
    /// Safe and useful to bulk-read: readable and not a device/vdso mapping.
    pub fn scannable(&self) -> bool {
        self.readable() && self.kind() != RegionKind::Special
    }
}

pub fn read_maps(pid: i32) -> std::io::Result<Vec<Region>> {
    let text = fs::read_to_string(format!("/proc/{pid}/maps"))?;
    Ok(text.lines().filter_map(parse_line).collect())
}

fn parse_line(line: &str) -> Option<Region> {
    let mut it = line.splitn(6, ' ');
    let range = it.next()?;
    let perms = it.next()?.to_string();
    let offset = u64::from_str_radix(it.next()?, 16).ok()?;
    let _dev = it.next()?;
    let inode = it.next()?.parse().ok()?;
    let path = it.next().unwrap_or("").trim_start().to_string();
    let (s, e) = range.split_once('-')?;
    Some(Region { start: u64::from_str_radix(s, 16).ok()?, end: u64::from_str_radix(e, 16).ok()?, perms, offset, inode, path })
}

/// Region containing `addr` (maps are sorted by start address).
pub fn region_for(regions: &[Region], addr: u64) -> Option<&Region> {
    let i = regions.partition_point(|r| r.end <= addr);
    regions.get(i).filter(|r| r.contains(addr))
}

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub name: String,
    pub path: String,
    pub base: u64,
    pub end: u64,
}

impl Module {
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.base && addr < self.end
    }
}

/// Group file-backed mappings into modules. An anonymous writable mapping that
/// directly follows a module's writable segment is its `.bss` and is folded in,
/// so static pointers living in `.bss` resolve to `module+offset`.
pub fn modules(regions: &[Region]) -> Vec<Module> {
    let mut out: Vec<Module> = Vec::new();
    let mut last_writable = false;
    for r in regions {
        match r.kind() {
            RegionKind::Image => {
                let extend = out.last().is_some_and(|m| m.path == r.path && r.offset != 0 && r.start >= m.end);
                if extend {
                    out.last_mut().unwrap().end = r.end;
                } else {
                    let name = r.path.rsplit('/').next().unwrap_or(&r.path).to_string();
                    out.push(Module { name, path: r.path.clone(), base: r.start, end: r.end });
                }
                last_writable = r.writable();
            }
            RegionKind::Anon if r.writable() && last_writable => {
                if let Some(m) = out.last_mut().filter(|m| m.end == r.start) {
                    m.end = r.end;
                }
                last_writable = false;
            }
            _ => last_writable = false,
        }
    }
    out
}

pub fn find_module<'a>(mods: &'a [Module], name: &str) -> Option<&'a Module> {
    mods.iter().find(|m| m.name.eq_ignore_ascii_case(name)).or_else(|| {
        // Allow "UnityPlayer" for "UnityPlayer.dll" when unambiguous.
        let mut hits = mods.iter().filter(|m| {
            m.name.len() > name.len() && m.name[..name.len()].eq_ignore_ascii_case(name) && m.name.as_bytes()[name.len()] == b'.'
        });
        let first = hits.next()?;
        hits.next().is_none().then_some(first)
    })
}

pub fn module_for(mods: &[Module], addr: u64) -> Option<&Module> {
    mods.iter().find(|m| m.contains(addr))
}

/// "Game.exe+0x1a2b" when `addr` is inside a module, else plain hex.
pub fn symbolize(mods: &[Module], addr: u64) -> String {
    match module_for(mods, addr) {
        Some(m) => format!("{}+0x{:x}", m.name, addr - m.base),
        None => crate::hex(addr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_groups() {
        let text = "\
55d000000000-55d000001000 r--p 00000000 103:02 42 /usr/bin/game
55d000001000-55d000003000 r-xp 00001000 103:02 42 /usr/bin/game
55d000003000-55d000004000 rw-p 00003000 103:02 42 /usr/bin/game
55d000004000-55d000006000 rw-p 00000000 00:00 0
55d001000000-55d002000000 rw-p 00000000 00:00 0                          [heap]
7ffd00000000-7ffd00021000 rw-p 00000000 00:00 0                          [stack]";
        let regions: Vec<Region> = text.lines().filter_map(parse_line).collect();
        assert_eq!(regions.len(), 6);
        assert_eq!(regions[4].kind(), RegionKind::Heap);
        let mods = modules(&regions);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "game");
        assert_eq!(mods[0].end, 0x55d000006000, ".bss folded into module");
        assert_eq!(symbolize(&mods, 0x55d000004010), "game+0x4010");
        assert!(region_for(&regions, 0x55d001000010).is_some());
        assert!(region_for(&regions, 0x55d000900000).is_none());
    }
}
