//! "Find what writes/accesses this address" using x86 debug registers.
//!
//! All threads are seized with `PTRACE_SEIZE` (no SIGSTOP side effects), DR0/DR7
//! are programmed, and the target keeps running while traps are collected.
//! Everything happens on the calling thread (ptrace is per-thread) and every
//! thread is detached with its debug registers cleared before returning.
//! If the target executes another program, the session ends at once: the new
//! image is detached before it runs a single instruction.

use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::mem::offset_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchKind {
    Write,
    Access,
}

#[derive(Debug, Clone, Serialize)]
pub struct Regs {
    pub rax: String,
    pub rbx: String,
    pub rcx: String,
    pub rdx: String,
    pub rsi: String,
    pub rdi: String,
    pub rbp: String,
    pub rsp: String,
    pub r8: String,
    pub r9: String,
    pub r10: String,
    pub r11: String,
    pub r12: String,
    pub r13: String,
    pub r14: String,
    pub r15: String,
}

impl Regs {
    fn from(r: &libc::user_regs_struct) -> Self {
        let h = crate::hex;
        Self {
            rax: h(r.rax),
            rbx: h(r.rbx),
            rcx: h(r.rcx),
            rdx: h(r.rdx),
            rsi: h(r.rsi),
            rdi: h(r.rdi),
            rbp: h(r.rbp),
            rsp: h(r.rsp),
            r8: h(r.r8),
            r9: h(r.r9),
            r10: h(r.r10),
            r11: h(r.r11),
            r12: h(r.r12),
            r13: h(r.r13),
            r14: h(r.r14),
            r15: h(r.r15),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawHit {
    /// Instruction pointer *after* the accessing instruction.
    pub rip_after: u64,
    pub count: u64,
    pub first_regs: Regs,
}

#[derive(Debug, Default)]
pub struct WatchReport {
    pub hits: Vec<RawHit>,
    pub threads: usize,
    pub elapsed_ms: u64,
}

const DR_OFFSET: usize = offset_of!(libc::user, u_debugreg);

fn dr(i: usize) -> usize {
    DR_OFFSET + i * 8
}

unsafe fn pt(req: libc::c_uint, tid: i32, addr: usize, data: usize) -> libc::c_long {
    libc::ptrace(req, tid, addr as *mut libc::c_void, data as *mut libc::c_void)
}

fn dr7_value(len: usize, kind: WatchKind) -> u64 {
    let rw: u64 = match kind {
        WatchKind::Write => 0b01,
        WatchKind::Access => 0b11,
    };
    let ln: u64 = match len {
        1 => 0b00,
        2 => 0b01,
        8 => 0b10,
        _ => 0b11,
    };
    1 | (rw << 16) | (ln << 18)
}

struct Session {
    addr: u64,
    dr7: u64,
    configured: HashMap<i32, bool>,
}

impl Session {
    /// Program DR0/DR7 on a stopped thread.
    fn arm(&mut self, tid: i32) -> bool {
        // SAFETY: tid is a ptrace-stopped tracee of this thread.
        let ok = unsafe {
            pt(libc::PTRACE_POKEUSER, tid, dr(0), self.addr as usize) == 0 && pt(libc::PTRACE_POKEUSER, tid, dr(7), self.dr7 as usize) == 0
        };
        self.configured.insert(tid, ok);
        ok
    }
}

fn disarm(tid: i32) {
    // SAFETY: tid is a ptrace-stopped tracee; failures are harmless.
    unsafe {
        pt(libc::PTRACE_POKEUSER, tid, dr(7), 0);
        pt(libc::PTRACE_POKEUSER, tid, dr(0), 0);
        pt(libc::PTRACE_POKEUSER, tid, dr(6), 0);
    }
}

/// Wait for a state change of one tracee, giving up after `timeout`.
/// `__WNOTHREAD` keeps us to tracees of this thread.
fn wait_tid(tid: i32, timeout: Duration) -> Option<i32> {
    let deadline = Instant::now() + timeout;
    let mut st = 0;
    loop {
        // SAFETY: non-blocking waitpid on one of our tracees.
        let r = unsafe { libc::waitpid(tid, &mut st, libc::__WALL | libc::__WNOTHREAD | libc::WNOHANG) };
        if r == tid {
            return Some(st);
        }
        let interrupted = r == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR);
        if (r == -1 && !interrupted) || Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_micros(200));
    }
}

/// Our trap? Reads and clears DR6.
fn take_hit(tid: i32) -> Option<libc::user_regs_struct> {
    // SAFETY: tid is stopped; PEEKUSER returns the register value (errno not needed: 0 is "no hit").
    let dr6 = unsafe { pt(libc::PTRACE_PEEKUSER, tid, dr(6), 0) } as u64;
    if dr6 & 1 == 0 {
        return None;
    }
    unsafe { pt(libc::PTRACE_POKEUSER, tid, dr(6), 0) };
    let mut regs: libc::user_regs_struct = unsafe { std::mem::zeroed() };
    // SAFETY: GETREGS writes a user_regs_struct into `regs`.
    if unsafe { pt(libc::PTRACE_GETREGS, tid, 0, &mut regs as *mut _ as usize) } != 0 {
        return None;
    }
    Some(regs)
}

fn record(hits: &mut BTreeMap<u64, RawHit>, total: &mut u64, regs: &libc::user_regs_struct) {
    *total += 1;
    hits.entry(regs.rip).and_modify(|h| h.count += 1).or_insert_with(|| RawHit {
        rip_after: regs.rip,
        count: 1,
        first_regs: Regs::from(regs),
    });
}

/// Runs the session on a dedicated thread. Tracees belong to the tracing thread,
/// so when it ends the kernel detaches every thread and drops the hardware
/// breakpoints even if our own cleanup could not reach a thread.
pub fn watch(
    pid: i32,
    addr: u64,
    len: usize,
    kind: WatchKind,
    duration: Duration,
    max_hits: u64,
    cancel: &AtomicBool,
) -> Result<WatchReport> {
    std::thread::scope(|s| {
        s.spawn(|| watch_on_this_thread(pid, addr, len, kind, duration, max_hits, cancel))
            .join()
            .unwrap_or_else(|_| Err(Error::new("internal", "watch thread panicked")))
    })
}

fn watch_on_this_thread(
    pid: i32,
    addr: u64,
    len: usize,
    kind: WatchKind,
    duration: Duration,
    max_hits: u64,
    cancel: &AtomicBool,
) -> Result<WatchReport> {
    if ![1, 2, 4, 8].contains(&len) {
        return Err(Error::invalid("size must be 1, 2, 4 or 8"));
    }
    if !addr.is_multiple_of(len as u64) {
        return Err(Error::invalid(format!("address must be {len}-byte aligned for a hardware watchpoint")));
    }
    let mut s = Session { addr, dr7: dr7_value(len, kind), configured: HashMap::new() };
    let mut hits: BTreeMap<u64, RawHit> = BTreeMap::new();
    let mut total = 0u64;

    // 1. Seize every thread, then arm them.
    let forbidden = || Error::new("forbidden", "the target is no longer an ordinary process of your user");
    if !crate::process::may_access(pid) {
        return Err(forbidden());
    }
    let tids = crate::process::threads(pid);
    if tids.is_empty() {
        return Err(Error::new("process_gone", "target has no threads"));
    }
    let mut tracked: Vec<i32> = Vec::new();
    let mut first_err = None;
    let options = (libc::PTRACE_O_TRACECLONE | libc::PTRACE_O_TRACEEXEC) as usize;
    crate::caps::with_ptrace(|| {
        for &tid in &tids {
            // SAFETY: seizing a thread of the target; checked return value.
            if unsafe { pt(libc::PTRACE_SEIZE, tid, 0, options) } != 0 {
                first_err.get_or_insert_with(std::io::Error::last_os_error);
                continue;
            }
            tracked.push(tid);
        }
    });
    // Seized but not yet touched: if it became privileged in between, returning
    // ends this thread and the kernel detaches everything.
    if !tracked.is_empty() && !crate::process::may_access(pid) {
        return Err(forbidden());
    }
    for &tid in &tracked {
        unsafe { pt(libc::PTRACE_INTERRUPT, tid, 0, 0) };
        let mut pending_sig = 0;
        if let Some(st) = wait_tid(tid, Duration::from_secs(2)) {
            if libc::WIFSTOPPED(st) && (st >> 16) == 0 && libc::WSTOPSIG(st) != libc::SIGTRAP {
                pending_sig = libc::WSTOPSIG(st);
            }
            if libc::WIFSTOPPED(st) {
                s.arm(tid);
                unsafe { pt(libc::PTRACE_CONT, tid, 0, pending_sig as usize) };
            }
        }
    }
    if tracked.is_empty() {
        let e = first_err.map(Error::from).unwrap_or_else(|| Error::new("permission_denied", "could not trace any thread"));
        return Err(Error::new(e.code, format!("{} (hardware watchpoints need ptrace access to the target)", e.message)));
    }
    let nthreads = tracked.len();

    // 2. Collect traps until the deadline.
    let mut executed = false;
    let start = Instant::now();
    let deadline = start + duration;
    while Instant::now() < deadline && total < max_hits && !cancel.load(Ordering::Relaxed) {
        let mut st = 0;
        // SAFETY: waits only for tracees of this thread (__WNOTHREAD).
        let tid = unsafe { libc::waitpid(-1, &mut st, libc::__WALL | libc::WNOHANG | libc::__WNOTHREAD) };
        if tid == 0 {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        if tid < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD) {
                break; // every thread is gone
            }
            continue;
        }
        if libc::WIFEXITED(st) || libc::WIFSIGNALED(st) {
            tracked.retain(|t| *t != tid);
            continue;
        }
        if !libc::WIFSTOPPED(st) {
            continue;
        }
        let event = st >> 16;
        let sig = libc::WSTOPSIG(st);
        let mut inject = 0;
        if event == libc::PTRACE_EVENT_EXEC {
            // A new program replaced the target; exec has already dropped the other
            // threads and the debug registers. Let it go untouched.
            unsafe { pt(libc::PTRACE_DETACH, tid, 0, 0) };
            tracked.clear();
            executed = true;
            break;
        }
        if event == libc::PTRACE_EVENT_CLONE {
            let mut new_tid: libc::c_ulong = 0;
            unsafe { pt(libc::PTRACE_GETEVENTMSG, tid, 0, &mut new_tid as *mut _ as usize) };
            if new_tid != 0 && !tracked.contains(&(new_tid as i32)) {
                tracked.push(new_tid as i32);
            }
        } else if event == libc::PTRACE_EVENT_STOP {
            if !s.configured.contains_key(&tid) {
                s.arm(tid); // a freshly cloned thread's first stop
                if !tracked.contains(&tid) {
                    tracked.push(tid);
                }
            }
        } else if sig == libc::SIGTRAP {
            match take_hit(tid) {
                Some(regs) => record(&mut hits, &mut total, &regs),
                None => inject = sig,
            }
        } else {
            inject = sig;
        }
        unsafe { pt(libc::PTRACE_CONT, tid, 0, inject as usize) };
    }

    // 3. Stop, disarm and detach every thread (including ones cloned meanwhile).
    let mut queue: VecDeque<i32> = tracked.into_iter().collect();
    let mut done: Vec<i32> = Vec::new();
    while let Some(tid) = queue.pop_front() {
        if done.contains(&tid) {
            continue;
        }
        done.push(tid);
        unsafe { pt(libc::PTRACE_INTERRUPT, tid, 0, 0) };
        let Some(st) = wait_tid(tid, Duration::from_secs(2)) else { continue };
        if !libc::WIFSTOPPED(st) {
            continue;
        }
        let event = st >> 16;
        let mut sig = 0;
        if event == libc::PTRACE_EVENT_CLONE {
            let mut new_tid: libc::c_ulong = 0;
            unsafe { pt(libc::PTRACE_GETEVENTMSG, tid, 0, &mut new_tid as *mut _ as usize) };
            if new_tid != 0 {
                queue.push_back(new_tid as i32);
            }
        } else if event == 0 {
            let s0 = libc::WSTOPSIG(st);
            if s0 == libc::SIGTRAP {
                if let Some(regs) = take_hit(tid) {
                    record(&mut hits, &mut total, &regs);
                } else {
                    sig = s0;
                }
            } else {
                sig = s0;
            }
        }
        disarm(tid);
        unsafe { pt(libc::PTRACE_DETACH, tid, 0, sig as usize) };
    }

    if executed {
        return Err(Error::new("process_gone", "the target executed another program; attach to it again"));
    }
    Ok(WatchReport { hits: hits.into_values().collect(), threads: nthreads, elapsed_ms: start.elapsed().as_millis() as u64 })
}
