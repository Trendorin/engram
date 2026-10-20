//! Semantic helpers built for agents: explain an address, guess a structure.

use crate::maps::{module_for, region_for, symbolize, Module, Region, RegionKind};
use crate::memory::ProcMem;
use crate::value::{hex_bytes, NumType};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct RegionSummary {
    pub start: String,
    pub end: String,
    pub perms: String,
    pub kind: RegionKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
}

impl RegionSummary {
    pub fn of(r: &Region) -> Self {
        Self { start: crate::hex(r.start), end: crate::hex(r.end), perms: r.perms.clone(), kind: r.kind(), path: r.path.clone() }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AddressInfo {
    pub address: String,
    pub location: String,
    pub region: Option<RegionSummary>,
    pub bytes: String,
    pub as_types: BTreeMap<&'static str, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointer_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Where does a pointer value point? "heap", "Game.exe+0x10", ... or None if unmapped.
pub fn pointee(regions: &[Region], mods: &[Module], v: u64) -> Option<String> {
    let r = region_for(regions, v)?;
    if !r.readable() {
        return None;
    }
    Some(match module_for(mods, v) {
        Some(_) => symbolize(mods, v),
        None => format!(
            "{} {}",
            crate::hex(v),
            match r.kind() {
                RegionKind::Heap => "[heap]",
                RegionKind::Stack => "[stack]",
                RegionKind::Anon => "[anon]",
                _ => "[mapped]",
            }
        ),
    })
}

fn printable(b: &[u8]) -> Option<String> {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    let s = &b[..end];
    (s.len() >= 3 && s.iter().all(|c| c.is_ascii_graphic() || *c == b' ')).then(|| String::from_utf8_lossy(s).into_owned())
}

pub fn describe(mem: &ProcMem, regions: &[Region], mods: &[Module], addr: u64, ptr_size: usize) -> crate::Result<AddressInfo> {
    let mut b = [0u8; 32];
    let n = mem.read(addr, &mut b)?;
    if n < 8 {
        return Err(crate::Error::new("unreadable", format!("{} is not readable", crate::hex(addr))));
    }
    let mut as_types = BTreeMap::new();
    for t in NumType::ALL {
        as_types.insert(t.name(), t.format(t.bits(&b)));
    }
    let pv =
        if ptr_size == 4 { u32::from_le_bytes(b[..4].try_into().unwrap()) as u64 } else { u64::from_le_bytes(b[..8].try_into().unwrap()) };
    let text = printable(&b[..n]).or_else(|| {
        let units: Vec<u16> = b[..n].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|u| *u != 0).collect();
        (units.len() >= 3 && units.iter().all(|u| (0x20..0x7f).contains(u))).then(|| format!("utf16: {}", String::from_utf16_lossy(&units)))
    });
    Ok(AddressInfo {
        address: crate::hex(addr),
        location: symbolize(mods, addr),
        region: region_for(regions, addr).map(RegionSummary::of),
        bytes: hex_bytes(&b[..16.min(n)]),
        as_types,
        pointer_to: pointee(regions, mods, pv),
        text,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub offset: String,
    pub guess: &'static str,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn plausible_f32(v: u32) -> bool {
    let f = f32::from_bits(v);
    f.is_finite() && f != 0.0 && (1e-4..1e7).contains(&f.abs())
}

fn plausible_f64(v: u64) -> bool {
    let f = f64::from_bits(v);
    f.is_finite() && f != 0.0 && (1e-6..1e12).contains(&f.abs())
}

/// Guess the layout of `size` bytes at `addr`: pointers, floats, ints, strings.
pub fn dissect(mem: &ProcMem, regions: &[Region], mods: &[Module], addr: u64, size: usize, ptr_size: usize) -> crate::Result<Vec<Field>> {
    let size = size.clamp(8, 4096);
    let mut b = vec![0u8; size];
    let n = mem.read(addr, &mut b)?;
    if n == 0 {
        return Err(crate::Error::new("unreadable", format!("{} is not readable", crate::hex(addr))));
    }
    b.truncate(n - n % 4);
    let mut out = Vec::new();
    let mut i = 0usize;
    let field = |off: usize, guess, value: String, note: Option<String>| Field { offset: format!("+0x{off:x}"), guess, value, note };
    while i + 4 <= b.len() {
        let u32v = u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        if (addr as usize + i).is_multiple_of(ptr_size) && i + ptr_size <= b.len() {
            let pv = if ptr_size == 4 { u32v as u64 } else { u64::from_le_bytes(b[i..i + 8].try_into().unwrap()) };
            if pv > 0x10000 {
                if let Some(target) = pointee(regions, mods, pv) {
                    out.push(field(i, "ptr", crate::hex(pv), Some(format!("-> {target}"))));
                    i += ptr_size;
                    continue;
                }
            }
            if ptr_size == 8 && i.is_multiple_of(8) {
                let hi = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap());
                if plausible_f64(pv) && !plausible_f32(u32v) && !plausible_f32(hi) {
                    out.push(field(i, "f64", NumType::F64.format(pv), None));
                    i += 8;
                    continue;
                }
            }
        }
        if u32v == 0 {
            let start = i;
            while i + 4 <= b.len() && b[i..i + 4] == [0, 0, 0, 0] {
                i += 4;
            }
            out.push(field(start, "zero", "0".into(), (i - start > 4).then(|| format!("{} bytes", i - start))));
            continue;
        }
        if let Some(s) = printable(&b[i..b.len().min(i + 64)]).filter(|s| s.len() >= 4) {
            let len = (s.len() + 1).div_ceil(4) * 4;
            out.push(field(i, "text", s, None));
            i += len;
            continue;
        }
        let iv = u32v as i32;
        let (guess, value, note) = if plausible_f32(u32v) && !(-65536..65536).contains(&iv) {
            ("f32", NumType::F32.format(u32v as u64), Some(format!("i32 {iv}")))
        } else if (-1_000_000..1_000_000).contains(&iv) {
            ("i32", iv.to_string(), plausible_f32(u32v).then(|| format!("f32 {}", f32::from_bits(u32v))))
        } else {
            ("hex", format!("0x{u32v:08x}"), None)
        };
        out.push(field(i, guess, value, note));
        i += 4;
    }
    Ok(out)
}

/// Pointer width of the main image: 4 for 32-bit PE (incl. WoW64) or ELF32, else 8.
pub fn pointer_size(mem: &ProcMem, mods: &[Module], exe_name: &str) -> usize {
    let main = mods.iter().find(|m| m.name.eq_ignore_ascii_case(exe_name)).or_else(|| mods.first());
    let Some(m) = main else { return 8 };
    let mut hdr = [0u8; 0x40];
    if mem.read_exact(m.base, &mut hdr).is_err() {
        return 8;
    }
    if &hdr[..4] == b"\x7fELF" {
        return if hdr[4] == 1 { 4 } else { 8 };
    }
    if &hdr[..2] == b"MZ" {
        let lfanew = u32::from_le_bytes(hdr[0x3c..0x40].try_into().unwrap()) as u64;
        let mut pe = [0u8; 6];
        if mem.read_exact(m.base + lfanew, &mut pe).is_ok() && &pe[..4] == b"PE\0\0" {
            return if u16::from_le_bytes([pe[4], pe[5]]) == 0x14c { 4 } else { 8 };
        }
    }
    8
}
