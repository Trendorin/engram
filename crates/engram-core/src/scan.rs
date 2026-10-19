//! Value scanner.
//!
//! Memory is processed in chunks of up to [`CHUNK`] bytes. For every chunk and
//! every scanned type ("kind") the candidates are stored either
//! * **dense**: a snapshot of the chunk plus a bitmask of candidate slots, or
//! * **sparse**: a list of `(offset, previous value)` hits,
//!
//! whichever is cheaper. Unknown-initial-value scans start fully dense and
//! collapse to sparse as they narrow. Several kinds can be scanned in a single
//! pass ("auto" type), sharing one snapshot per chunk.

use crate::maps::Region;
use crate::memory::ProcMem;
use crate::value::{parse_pattern, NumType};
use crate::{Error, Result};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub const CHUNK: usize = 4 << 20;
/// Cap for pattern/string matches; they are always stored sparse.
pub const MAX_PATTERN_HITS: usize = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compare {
    Exact,
    NotEqual,
    Greater,
    Less,
    Between,
    Unknown,
    Changed,
    Unchanged,
    Increased,
    Decreased,
    IncreasedBy,
    DecreasedBy,
}

impl Compare {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "exact" | "eq" | "=" | "==" | "equal" => Self::Exact,
            "not_equal" | "ne" | "!=" => Self::NotEqual,
            "greater" | "gt" | ">" | "bigger" | "bigger_than" => Self::Greater,
            "less" | "lt" | "<" | "smaller" | "smaller_than" => Self::Less,
            "between" | "range" => Self::Between,
            "unknown" | "unknown_initial" | "any" => Self::Unknown,
            "changed" => Self::Changed,
            "unchanged" | "same" => Self::Unchanged,
            "increased" | "inc" => Self::Increased,
            "decreased" | "dec" => Self::Decreased,
            "increased_by" | "inc_by" => Self::IncreasedBy,
            "decreased_by" | "dec_by" => Self::DecreasedBy,
            other => return Err(Error::invalid(format!("unknown compare '{other}'"))),
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::NotEqual => "not_equal",
            Self::Greater => "greater",
            Self::Less => "less",
            Self::Between => "between",
            Self::Unknown => "unknown",
            Self::Changed => "changed",
            Self::Unchanged => "unchanged",
            Self::Increased => "increased",
            Self::Decreased => "decreased",
            Self::IncreasedBy => "increased_by",
            Self::DecreasedBy => "decreased_by",
        }
    }

    pub fn needs_previous(self) -> bool {
        matches!(self, Self::Changed | Self::Unchanged | Self::Increased | Self::Decreased | Self::IncreasedBy | Self::DecreasedBy)
    }

    pub fn value_count(self) -> usize {
        match self {
            Self::Between => 2,
            Self::Exact | Self::NotEqual | Self::Greater | Self::Less | Self::IncreasedBy | Self::DecreasedBy => 1,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternFlavor {
    Bytes,
    Utf8,
    Utf16,
}

#[derive(Debug, Clone)]
pub enum ScanKind {
    Num(NumType),
    Pattern { flavor: PatternFlavor, bytes: Vec<u8>, mask: Vec<u8> },
}

impl ScanKind {
    pub fn pattern(flavor: PatternFlavor, text: &str) -> Result<Self> {
        let (bytes, mask) = match flavor {
            PatternFlavor::Bytes => parse_pattern(text)?,
            PatternFlavor::Utf8 => (text.as_bytes().to_vec(), vec![0xFF; text.len()]),
            PatternFlavor::Utf16 => {
                let b: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
                let m = vec![0xFF; b.len()];
                (b, m)
            }
        };
        if bytes.is_empty() {
            return Err(Error::invalid("empty search text"));
        }
        Ok(Self::Pattern { flavor, bytes, mask })
    }

    pub fn name(&self) -> String {
        match self {
            Self::Num(n) => n.name().to_string(),
            Self::Pattern { flavor: PatternFlavor::Bytes, .. } => "bytes".into(),
            Self::Pattern { flavor: PatternFlavor::Utf8, .. } => "string".into(),
            Self::Pattern { flavor: PatternFlavor::Utf16, .. } => "string16".into(),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Self::Num(n) => n.size(),
            Self::Pattern { bytes, .. } => bytes.len(),
        }
    }

    fn align(&self, aligned: bool) -> usize {
        match self {
            Self::Num(n) if aligned => n.align(),
            _ => 1,
        }
    }
}

/// Pick plausible types for a value whose type the user does not know.
pub fn auto_kinds(compare: Compare, value: Option<&str>) -> Vec<NumType> {
    use NumType::*;
    let v = value.map(str::trim).unwrap_or("");
    if compare == Compare::Unknown || v.is_empty() {
        return vec![I32, F32, F64];
    }
    if v.contains(['.', 'e', 'E']) && !v.starts_with("0x") {
        return vec![F32, F64];
    }
    match crate::value::parse_int(v) {
        Ok(i) if i.abs() > i32::MAX as i128 => vec![I64, F64],
        _ => vec![I32, I64, F32, F64],
    }
}

#[derive(Debug, Clone)]
pub struct Query {
    pub compare: Compare,
    pub value: Option<String>,
    pub value2: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub writable_only: bool,
    pub start: u64,
    pub end: u64,
}

impl Default for Scope {
    fn default() -> Self {
        Self { writable_only: true, start: 0, end: u64::MAX }
    }
}

#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn reset(&self, total: u64) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        self.cancel.store(false, Ordering::Relaxed);
    }
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Typed predicates

trait Scalar: Copy + PartialOrd + Send + Sync + 'static {
    const SIZE: usize;
    const FLOAT: bool;
    /// # Safety
    /// `p` must be valid for `SIZE` bytes.
    unsafe fn load(p: *const u8) -> Self;
    fn from_bits(b: u64) -> Self;
    fn to_bits(self) -> u64;
    fn from_f64(v: f64) -> Self;
    fn neg(self) -> Self;
    fn delta(new: Self, old: Self) -> Self;
}

macro_rules! int_scalar {
    ($t:ty, $u:ty) => {
        impl Scalar for $t {
            const SIZE: usize = std::mem::size_of::<$t>();
            const FLOAT: bool = false;
            #[inline(always)]
            unsafe fn load(p: *const u8) -> Self {
                (p as *const $t).read_unaligned()
            }
            #[inline(always)]
            fn from_bits(b: u64) -> Self {
                b as $u as $t
            }
            #[inline(always)]
            fn to_bits(self) -> u64 {
                self as $u as u64
            }
            fn from_f64(v: f64) -> Self {
                v as $t
            }
            fn neg(self) -> Self {
                self.wrapping_neg()
            }
            #[inline(always)]
            fn delta(new: Self, old: Self) -> Self {
                new.wrapping_sub(old)
            }
        }
    };
}
int_scalar!(i8, u8);
int_scalar!(i16, u16);
int_scalar!(i32, u32);
int_scalar!(i64, u64);
int_scalar!(u8, u8);
int_scalar!(u16, u16);
int_scalar!(u32, u32);
int_scalar!(u64, u64);

macro_rules! float_scalar {
    ($t:ty, $u:ty) => {
        impl Scalar for $t {
            const SIZE: usize = std::mem::size_of::<$t>();
            const FLOAT: bool = true;
            #[inline(always)]
            unsafe fn load(p: *const u8) -> Self {
                (p as *const $t).read_unaligned()
            }
            #[inline(always)]
            fn from_bits(b: u64) -> Self {
                <$t>::from_bits(b as $u)
            }
            #[inline(always)]
            fn to_bits(self) -> u64 {
                <$t>::to_bits(self) as u64
            }
            fn from_f64(v: f64) -> Self {
                v as $t
            }
            fn neg(self) -> Self {
                -self
            }
            #[inline(always)]
            fn delta(new: Self, old: Self) -> Self {
                new - old
            }
        }
    };
}
float_scalar!(f32, u32);
float_scalar!(f64, u64);

macro_rules! with_type {
    ($nt:expr, $T:ident, $body:block) => {
        match $nt {
            NumType::I8 => {
                type $T = i8;
                $body
            }
            NumType::I16 => {
                type $T = i16;
                $body
            }
            NumType::I32 => {
                type $T = i32;
                $body
            }
            NumType::I64 => {
                type $T = i64;
                $body
            }
            NumType::U8 => {
                type $T = u8;
                $body
            }
            NumType::U16 => {
                type $T = u16;
                $body
            }
            NumType::U32 => {
                type $T = u32;
                $body
            }
            NumType::U64 => {
                type $T = u64;
                $body
            }
            NumType::F32 => {
                type $T = f32;
                $body
            }
            NumType::F64 => {
                type $T = f64;
                $body
            }
        }
    };
}

#[derive(Clone, Copy)]
enum Pred<T> {
    All,
    Eq(T),
    Ne(T),
    In(T, T),
    Out(T, T),
    Gt(T),
    Lt(T),
    Changed,
    Unchanged,
    Inc,
    Dec,
    Delta(T),
    DeltaIn(T, T),
}

impl<T: Scalar> Pred<T> {
    #[inline(always)]
    fn test(&self, new: T, old: T) -> bool {
        match *self {
            Pred::All => true,
            Pred::Eq(v) => new == v,
            Pred::Ne(v) => new != v,
            Pred::In(a, b) => new >= a && new <= b,
            Pred::Out(a, b) => !(new >= a && new <= b),
            Pred::Gt(v) => new > v,
            Pred::Lt(v) => new < v,
            Pred::Changed => new.to_bits() != old.to_bits(),
            Pred::Unchanged => new.to_bits() == old.to_bits(),
            Pred::Inc => new > old,
            Pred::Dec => new < old,
            Pred::Delta(d) => T::delta(new, old) == d,
            Pred::DeltaIn(a, b) => {
                let d = T::delta(new, old);
                d >= a && d <= b
            }
        }
    }
}

/// Half-width of the "rounded" match window implied by how many decimals the
/// user typed: "100" matches 99.5..100.5, "1.25" matches 1.245..1.255.
fn rounding_half(text: &str) -> f64 {
    let t = text.trim();
    if t.contains(['e', 'E']) {
        return t.parse::<f64>().map(|v| v.abs() * 1e-6).unwrap_or(0.0);
    }
    let decimals = t.split_once('.').map(|(_, d)| d.len()).unwrap_or(0) as i32;
    0.5 * 10f64.powi(-decimals)
}

/// (exact, lo, hi) for a typed literal.
fn literal<T: Scalar>(nt: NumType, text: &str) -> Result<(T, T, T)> {
    if T::FLOAT {
        let v: f64 = text.trim().parse().map_err(|_| Error::invalid(format!("'{text}' is not a number")))?;
        let h = rounding_half(text);
        Ok((T::from_f64(v), T::from_f64(v - h), T::from_f64(v + h)))
    } else {
        let b = nt.encode(text)?;
        let v = T::from_bits(nt.bits(&b));
        Ok((v, v, v))
    }
}

fn compile<T: Scalar>(nt: NumType, q: &Query) -> Result<Pred<T>> {
    let v1 = || q.value.as_deref().ok_or_else(|| Error::invalid(format!("compare '{}' needs a value", q.compare.name())));
    Ok(match q.compare {
        Compare::Unknown => Pred::All,
        Compare::Changed => Pred::Changed,
        Compare::Unchanged => Pred::Unchanged,
        Compare::Increased => Pred::Inc,
        Compare::Decreased => Pred::Dec,
        Compare::Exact => {
            let (v, lo, hi) = literal::<T>(nt, v1()?)?;
            if T::FLOAT {
                Pred::In(lo, hi)
            } else {
                Pred::Eq(v)
            }
        }
        Compare::NotEqual => {
            let (v, lo, hi) = literal::<T>(nt, v1()?)?;
            if T::FLOAT {
                Pred::Out(lo, hi)
            } else {
                Pred::Ne(v)
            }
        }
        Compare::Greater => Pred::Gt(literal::<T>(nt, v1()?)?.0),
        Compare::Less => Pred::Lt(literal::<T>(nt, v1()?)?.0),
        Compare::Between => {
            let a = literal::<T>(nt, v1()?)?.0;
            let b = literal::<T>(nt, q.value2.as_deref().ok_or_else(|| Error::invalid("between needs value2"))?)?.0;
            if a <= b {
                Pred::In(a, b)
            } else {
                Pred::In(b, a)
            }
        }
        Compare::IncreasedBy => {
            let (v, lo, hi) = literal::<T>(nt, v1()?)?;
            if T::FLOAT {
                Pred::DeltaIn(lo, hi)
            } else {
                Pred::Delta(v)
            }
        }
        Compare::DecreasedBy => {
            let (v, lo, hi) = literal::<T>(nt, v1()?)?;
            if T::FLOAT {
                Pred::DeltaIn(hi.neg(), lo.neg())
            } else {
                Pred::Delta(v.neg())
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Storage

#[derive(Clone, Copy, Debug)]
struct Hit {
    off: u32,
    prev: u64,
}

#[derive(Clone)]
enum Slot {
    Empty,
    Dense { mask: Vec<u64>, count: u64 },
    Sparse(Vec<Hit>),
}

impl Slot {
    fn count(&self) -> u64 {
        match self {
            Slot::Empty => 0,
            Slot::Dense { count, .. } => *count,
            Slot::Sparse(h) => h.len() as u64,
        }
    }
}

#[derive(Clone)]
struct Chunk {
    base: u64,
    data: Option<Arc<Vec<u8>>>,
    slots: Vec<Slot>,
}

fn slot_count(len: usize, size: usize, align: usize) -> usize {
    if len < size {
        0
    } else {
        (len - size) / align + 1
    }
}

fn iter_mask(mask: &[u64]) -> impl Iterator<Item = usize> + '_ {
    mask.iter().enumerate().flat_map(|(w, &bits)| {
        let mut b = bits;
        std::iter::from_fn(move || {
            if b == 0 {
                return None;
            }
            let t = b.trailing_zeros() as usize;
            b &= b - 1;
            Some(w * 64 + t)
        })
    })
}

/// Keep a dense mask, or convert it to a sparse list when that is smaller.
fn shape(kind: &ScanKind, align: usize, buf: &[u8], mask: Vec<u64>, count: u64) -> Slot {
    if count == 0 {
        return Slot::Empty;
    }
    if (count as usize) * 128 <= buf.len() {
        let size = kind.size();
        let hits = iter_mask(&mask)
            .map(|i| {
                let off = i * align;
                let prev = match kind {
                    ScanKind::Num(n) => n.bits(&buf[off..off + size]),
                    ScanKind::Pattern { .. } => 0,
                };
                Hit { off: off as u32, prev }
            })
            .collect();
        Slot::Sparse(hits)
    } else {
        Slot::Dense { mask, count }
    }
}

fn first_num<T: Scalar>(buf: &[u8], align: usize, pred: Pred<T>) -> (Vec<u64>, u64) {
    let n = slot_count(buf.len(), T::SIZE, align);
    let mut mask = vec![0u64; n.div_ceil(64)];
    if let Pred::All = pred {
        for (w, word) in mask.iter_mut().enumerate() {
            let left = n - w * 64;
            *word = if left >= 64 { u64::MAX } else { (1u64 << left) - 1 };
        }
        return (mask, n as u64);
    }
    let p = buf.as_ptr();
    let mut count = 0u64;
    for i in 0..n {
        // SAFETY: i * align + SIZE <= buf.len() by construction of `n`.
        let v = unsafe { T::load(p.add(i * align)) };
        if pred.test(v, v) {
            mask[i >> 6] |= 1 << (i & 63);
            count += 1;
        }
    }
    (mask, count)
}

fn next_dense<T: Scalar>(old: &[u8], new: &[u8], align: usize, mask: &[u64], pred: Pred<T>) -> (Vec<u64>, u64) {
    let limit = slot_count(new.len().min(old.len()), T::SIZE, align);
    let mut out = vec![0u64; mask.len()];
    let mut count = 0;
    for i in iter_mask(mask) {
        if i >= limit {
            break;
        }
        let off = i * align;
        // SAFETY: off + SIZE <= min(old.len(), new.len()).
        let (o, n) = unsafe { (T::load(old.as_ptr().add(off)), T::load(new.as_ptr().add(off))) };
        if pred.test(n, o) {
            out[i >> 6] |= 1 << (i & 63);
            count += 1;
        }
    }
    (out, count)
}

fn pattern_matches(buf: &[u8], bytes: &[u8], mask: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    if buf.len() < bytes.len() {
        return out;
    }
    if mask.iter().all(|m| *m == 0xFF) {
        out.extend(memchr::memmem::find_iter(buf, bytes).map(|o| o as u32));
        return out;
    }
    let anchor = mask.iter().position(|m| *m == 0xFF).unwrap();
    let last_start = buf.len() - bytes.len();
    for pos in memchr::memchr_iter(bytes[anchor], &buf[anchor..=last_start + anchor]) {
        let start = pos;
        if (0..bytes.len()).all(|i| mask[i] == 0 || buf[start + i] == bytes[i]) {
            out.push(start as u32);
        }
    }
    out
}

fn pattern_eq(window: &[u8], bytes: &[u8], mask: &[u8]) -> bool {
    window.len() >= bytes.len() && (0..bytes.len()).all(|i| mask[i] == 0 || window[i] == bytes[i])
}

// ---------------------------------------------------------------------------
// Session

#[derive(Clone)]
pub struct ScanSession {
    pub kinds: Vec<ScanKind>,
    pub aligned: bool,
    pub counts: Vec<u64>,
    pub step: u32,
    pub scanned_bytes: u64,
    pub last_compare: Compare,
    chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, Copy)]
pub struct HitRef {
    pub addr: u64,
    pub kind: usize,
    pub prev: u64,
}

pub fn plan(regions: &[Region], scope: &Scope) -> Vec<(u64, usize)> {
    let mut out = Vec::new();
    for r in regions.iter().filter(|r| r.scannable() && (!scope.writable_only || r.writable())) {
        let (s, e) = (r.start.max(scope.start), r.end.min(scope.end));
        let mut a = s;
        while a < e {
            let len = (e - a).min(CHUNK as u64);
            out.push((a, len as usize));
            a += len;
        }
    }
    out
}

impl ScanSession {
    pub fn first(
        mem: &ProcMem,
        regions: &[Region],
        kinds: Vec<ScanKind>,
        aligned: bool,
        q: &Query,
        scope: &Scope,
        progress: &Progress,
    ) -> Result<Self> {
        if kinds.is_empty() {
            return Err(Error::invalid("no scan types"));
        }
        if q.compare.needs_previous() {
            return Err(Error::invalid(format!(
                "'{}' compares against a previous scan; start with 'unknown' or 'exact'",
                q.compare.name()
            )));
        }
        // Validate literals once, up front, so errors are reported instead of silently matching nothing.
        let mut usable = Vec::new();
        let mut last_err = None;
        for k in kinds {
            let ok = match &k {
                ScanKind::Num(nt) => with_type!(*nt, T, { compile::<T>(*nt, q).map(|_| ()) }),
                ScanKind::Pattern { .. } => Ok(()),
            };
            match ok {
                Ok(()) => usable.push(k),
                Err(e) => last_err = Some(e),
            }
        }
        if usable.is_empty() {
            return Err(last_err.unwrap());
        }
        let kinds = usable;
        let specs = plan(regions, scope);
        progress.reset(specs.iter().map(|s| s.1 as u64).sum());
        let pattern_hits = AtomicU64::new(0);

        let chunks: Vec<Chunk> = specs
            .par_iter()
            .filter_map(|&(base, len)| {
                if progress.cancelled() {
                    return None;
                }
                let mut buf = vec![0u8; len];
                let n = mem.read(base, &mut buf).unwrap_or(0);
                progress.done.fetch_add(len as u64, Ordering::Relaxed);
                if n == 0 {
                    return None;
                }
                buf.truncate(n);
                let slots = kinds
                    .iter()
                    .map(|k| match k {
                        ScanKind::Num(nt) => {
                            let align = k.align(aligned);
                            let (mask, count) = with_type!(*nt, T, {
                                match compile::<T>(*nt, q) {
                                    Ok(p) => first_num::<T>(&buf, align, p),
                                    Err(_) => (Vec::new(), 0),
                                }
                            });
                            shape(k, align, &buf, mask, count)
                        }
                        ScanKind::Pattern { bytes, mask, .. } => {
                            if pattern_hits.load(Ordering::Relaxed) as usize >= MAX_PATTERN_HITS {
                                return Slot::Empty;
                            }
                            let offs = pattern_matches(&buf, bytes, mask);
                            pattern_hits.fetch_add(offs.len() as u64, Ordering::Relaxed);
                            if offs.is_empty() {
                                Slot::Empty
                            } else {
                                Slot::Sparse(offs.into_iter().map(|off| Hit { off, prev: 0 }).collect())
                            }
                        }
                    })
                    .collect();
                finalize(base, buf, slots)
            })
            .collect();
        if progress.cancelled() {
            return Err(Error::new("cancelled", "scan cancelled"));
        }
        Ok(Self::assemble(kinds, aligned, chunks, 1, progress.total.load(Ordering::Relaxed), q.compare))
    }

    fn assemble(kinds: Vec<ScanKind>, aligned: bool, chunks: Vec<Chunk>, step: u32, bytes: u64, c: Compare) -> Self {
        let counts = (0..kinds.len()).map(|k| chunks.iter().map(|ch| ch.slots[k].count()).sum()).collect();
        Self { kinds, aligned, counts, step, scanned_bytes: bytes, last_compare: c, chunks }
    }

    pub fn total(&self) -> u64 {
        self.counts.iter().sum()
    }

    /// Approximate resident memory used by the session.
    pub fn memory_bytes(&self) -> u64 {
        self.chunks
            .iter()
            .map(|c| {
                c.data.as_ref().map_or(0, |d| d.len() as u64)
                    + c.slots
                        .iter()
                        .map(|s| match s {
                            Slot::Empty => 0,
                            Slot::Dense { mask, .. } => mask.len() as u64 * 8,
                            Slot::Sparse(h) => h.len() as u64 * 16,
                        })
                        .sum::<u64>()
            })
            .sum()
    }

    pub fn next(&self, mem: &ProcMem, q: &Query, progress: &Progress) -> Result<Self> {
        if q.compare == Compare::Unknown {
            return Err(Error::invalid("'unknown' is only valid for a first scan"));
        }
        let mut kinds = self.kinds.clone();
        // Patterns re-check with the new text if given, else the original one.
        for k in kinds.iter_mut() {
            if let ScanKind::Pattern { flavor, .. } = k {
                if let Some(v) = q.value.as_deref().filter(|_| q.compare == Compare::Exact) {
                    *k = ScanKind::pattern(*flavor, v)?;
                }
            }
        }
        // In multi-type sessions a literal may not fit every type ("5.5" for i32):
        // those types simply drop out. Fail only if no type accepts it.
        let mut errors = kinds.iter().filter_map(|k| match k {
            ScanKind::Num(nt) => with_type!(*nt, T, { compile::<T>(*nt, q).err() }),
            ScanKind::Pattern { .. } => None,
        });
        let num_kinds = kinds.iter().filter(|k| matches!(k, ScanKind::Num(_))).count();
        if let Some(first) = errors.next() {
            if 1 + errors.count() == num_kinds && num_kinds == kinds.len() {
                return Err(first);
            }
        }
        progress.reset(self.chunks.iter().map(|c| c.data.as_ref().map_or(4096, |d| d.len() as u64)).sum());
        let aligned = self.aligned;
        let chunks: Vec<Chunk> = self
            .chunks
            .par_iter()
            .filter_map(|c| {
                if progress.cancelled() {
                    return None;
                }
                let r = next_chunk(c, &kinds, aligned, mem, q);
                progress.done.fetch_add(c.data.as_ref().map_or(4096, |d| d.len() as u64), Ordering::Relaxed);
                r
            })
            .collect();
        if progress.cancelled() {
            return Err(Error::new("cancelled", "scan cancelled"));
        }
        Ok(Self::assemble(kinds, aligned, chunks, self.step + 1, self.scanned_bytes, q.compare))
    }

    /// Results in (kind, address) order. `skip`/`limit` page through them.
    pub fn hits(&self, kind: Option<usize>, mut skip: u64, limit: usize) -> Vec<HitRef> {
        let mut out = Vec::new();
        for k in 0..self.kinds.len() {
            if kind.is_some_and(|f| f != k) {
                continue;
            }
            let size = self.kinds[k].size();
            let align = self.kinds[k].align(self.aligned);
            for c in &self.chunks {
                if out.len() >= limit {
                    return out;
                }
                let slot = &c.slots[k];
                let n = slot.count();
                if skip >= n {
                    skip -= n;
                    continue;
                }
                let want = limit - out.len();
                match slot {
                    Slot::Empty => {}
                    Slot::Sparse(h) => out.extend(h.iter().skip(skip as usize).take(want).map(|h| HitRef {
                        addr: c.base + h.off as u64,
                        kind: k,
                        prev: h.prev,
                    })),
                    Slot::Dense { mask, .. } => {
                        let data = c.data.as_ref().expect("dense slot has data");
                        out.extend(iter_mask(mask).skip(skip as usize).take(want).map(|i| {
                            let off = i * align;
                            let prev = match &self.kinds[k] {
                                ScanKind::Num(nt) => nt.bits(&data[off..off + size]),
                                _ => 0,
                            };
                            HitRef { addr: c.base + off as u64, kind: k, prev }
                        }))
                    }
                }
                skip = 0;
            }
        }
        out
    }
}

fn finalize(base: u64, buf: Vec<u8>, slots: Vec<Slot>) -> Option<Chunk> {
    if slots.iter().all(|s| matches!(s, Slot::Empty)) {
        return None;
    }
    let dense = slots.iter().any(|s| matches!(s, Slot::Dense { .. }));
    Some(Chunk { base, data: dense.then(|| Arc::new(buf)), slots })
}

fn next_chunk(c: &Chunk, kinds: &[ScanKind], aligned: bool, mem: &ProcMem, q: &Query) -> Option<Chunk> {
    // Fresh bytes: the whole chunk if any slot is dense, else a span covering the
    // sparse hits when it is compact enough, else per-hit scattered reads.
    let (fresh, fresh_off): (Option<Vec<u8>>, usize) = if let Some(old) = &c.data {
        let mut buf = vec![0u8; old.len()];
        let n = mem.read(c.base, &mut buf).unwrap_or(0);
        if n == 0 {
            return None;
        }
        buf.truncate(n);
        (Some(buf), 0)
    } else {
        let mut lo = usize::MAX;
        let mut hi = 0usize;
        let mut total = 0usize;
        for (k, s) in c.slots.iter().enumerate() {
            if let Slot::Sparse(h) = s {
                if let (Some(f), Some(l)) = (h.first(), h.last()) {
                    lo = lo.min(f.off as usize);
                    hi = hi.max(l.off as usize + kinds[k].size());
                    total += h.len();
                }
            }
        }
        if total == 0 {
            return None;
        }
        let span = hi - lo;
        if span <= 256 * 1024 || span <= total * 256 {
            let mut buf = vec![0u8; span];
            let n = mem.read(c.base + lo as u64, &mut buf).unwrap_or(0);
            buf.truncate(n);
            (Some(buf), lo)
        } else {
            (None, 0)
        }
    };

    let slots: Vec<Slot> = c
        .slots
        .iter()
        .enumerate()
        .map(|(k, slot)| {
            let kind = &kinds[k];
            let size = kind.size();
            match slot {
                Slot::Empty => Slot::Empty,
                Slot::Dense { mask, .. } => {
                    let (old, new) = (c.data.as_ref().unwrap(), fresh.as_ref().unwrap());
                    let ScanKind::Num(nt) = kind else { return Slot::Empty };
                    let align = kind.align(aligned);
                    let (m, count) = with_type!(*nt, T, {
                        match compile::<T>(*nt, q) {
                            Ok(p) => next_dense::<T>(old, new, align, mask, p),
                            Err(_) => (Vec::new(), 0),
                        }
                    });
                    shape(kind, align, new, m, count)
                }
                Slot::Sparse(hits) => {
                    // Gather fresh windows for every hit.
                    let mut scratch = Vec::new();
                    let mut ok = Vec::new();
                    if fresh.is_none() {
                        let addrs: Vec<u64> = hits.iter().map(|h| c.base + h.off as u64).collect();
                        scratch = vec![0u8; addrs.len() * size];
                        ok = mem.read_scatter(&addrs, size, &mut scratch);
                    }
                    let window = |i: usize, h: &Hit| -> Option<&[u8]> {
                        match &fresh {
                            Some(buf) => {
                                let s = h.off as usize - fresh_off;
                                buf.get(s..s + size)
                            }
                            None => ok[i].then(|| &scratch[i * size..(i + 1) * size]),
                        }
                    };
                    let kept: Vec<Hit> = match kind {
                        ScanKind::Num(nt) => with_type!(*nt, T, {
                            match compile::<T>(*nt, q) {
                                Ok(p) => hits
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(i, h)| {
                                        let w = window(i, h)?;
                                        let bits = nt.bits(w);
                                        p.test(<T as Scalar>::from_bits(bits), <T as Scalar>::from_bits(h.prev))
                                            .then_some(Hit { off: h.off, prev: bits })
                                    })
                                    .collect(),
                                Err(_) => Vec::new(),
                            }
                        }),
                        ScanKind::Pattern { bytes, mask, .. } => hits
                            .iter()
                            .enumerate()
                            .filter(|(i, h)| window(*i, h).is_some_and(|w| pattern_eq(w, bytes, mask)))
                            .map(|(_, h)| *h)
                            .collect(),
                    };
                    if kept.is_empty() {
                        Slot::Empty
                    } else {
                        Slot::Sparse(kept)
                    }
                }
            }
        })
        .collect();

    if slots.iter().all(|s| matches!(s, Slot::Empty)) {
        return None;
    }
    let dense = slots.iter().any(|s| matches!(s, Slot::Dense { .. }));
    Some(Chunk { base: c.base, data: if dense { fresh.map(Arc::new) } else { None }, slots })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own_region(buf: &[u8]) -> Vec<Region> {
        let start = buf.as_ptr() as u64;
        vec![Region { start, end: start + buf.len() as u64, perms: "rw-p".into(), offset: 0, inode: 0, path: String::new() }]
    }

    fn q(c: Compare, v: Option<&str>) -> Query {
        Query { compare: c, value: v.map(String::from), value2: None }
    }

    #[test]
    fn exact_then_next() {
        let mut data = vec![0u8; 64 * 1024];
        for (i, at) in [128usize, 4096, 40000].iter().enumerate() {
            data[*at..*at + 4].copy_from_slice(&(1234i32 + if i == 2 { 1 } else { 0 }).to_le_bytes());
        }
        let regions = own_region(&data);
        let mem = ProcMem::open(std::process::id() as i32).unwrap();
        let p = Progress::default();
        let s = ScanSession::first(
            &mem,
            &regions,
            vec![ScanKind::Num(NumType::I32)],
            true,
            &q(Compare::Exact, Some("1234")),
            &Scope::default(),
            &p,
        )
        .unwrap();
        assert_eq!(s.total(), 2);
        data[4096..4100].copy_from_slice(&1200i32.to_le_bytes());
        let s2 = s.next(&mem, &q(Compare::Decreased, None), &p).unwrap();
        assert_eq!(s2.total(), 1);
        assert_eq!(s2.hits(None, 0, 10)[0].addr, data.as_ptr() as u64 + 4096);
        std::hint::black_box(&data);
    }

    #[test]
    fn unknown_dense_collapses() {
        let mut data = vec![7u8; 256 * 1024];
        let regions = own_region(&data);
        let mem = ProcMem::open(std::process::id() as i32).unwrap();
        let p = Progress::default();
        let kinds = vec![ScanKind::Num(NumType::I32), ScanKind::Num(NumType::F32)];
        let s = ScanSession::first(&mem, &regions, kinds, true, &q(Compare::Unknown, None), &Scope::default(), &p).unwrap();
        assert_eq!(s.counts[0], (256 * 1024 - 4) / 4 + 1);
        data[1000..1004].copy_from_slice(&5.5f32.to_le_bytes());
        let s2 = s.next(&mem, &q(Compare::Changed, None), &p).unwrap();
        assert_eq!(s2.counts, vec![1, 1]);
        let s3 = s2.next(&mem, &q(Compare::Exact, Some("5.5")), &p).unwrap();
        assert_eq!(s3.counts, vec![0, 1]);
        std::hint::black_box(&data);
    }

    #[test]
    fn float_rounding_and_patterns() {
        let mut data = vec![0u8; 8192];
        data[64..68].copy_from_slice(&99.7f32.to_le_bytes());
        data[300..311].copy_from_slice(b"hello world");
        let regions = own_region(&data);
        let mem = ProcMem::open(std::process::id() as i32).unwrap();
        let p = Progress::default();
        let s = ScanSession::first(
            &mem,
            &regions,
            vec![ScanKind::Num(NumType::F32)],
            true,
            &q(Compare::Exact, Some("100")),
            &Scope::default(),
            &p,
        )
        .unwrap();
        assert_eq!(s.total(), 1);
        let pat = ScanKind::pattern(PatternFlavor::Bytes, "6c 6f ?? 77").unwrap();
        let s = ScanSession::first(&mem, &regions, vec![pat], false, &q(Compare::Exact, None), &Scope::default(), &p).unwrap();
        assert_eq!(s.hits(None, 0, 5)[0].addr, data.as_ptr() as u64 + 303);
        std::hint::black_box(&data);
    }
}
