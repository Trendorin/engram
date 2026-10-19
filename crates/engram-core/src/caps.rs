//! CAP_SYS_PTRACE stays out of the effective set except around the two calls
//! that need it: opening `/proc/<pid>/mem` and `PTRACE_SEIZE`.
//!
//! Capabilities are per thread. `engramd` calls [`lower`] on its main thread
//! before it starts any other thread, so every thread begins without the
//! capability; [`with_ptrace`] raises it only on the calling thread.

const CAP_SYS_PTRACE: u32 = 19;
const VERSION_3: u32 = 0x2008_0522;

#[repr(C)]
struct Header {
    version: u32,
    pid: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Data {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}

fn get() -> Option<[Data; 2]> {
    let mut h = Header { version: VERSION_3, pid: 0 };
    let mut d = [Data::default(); 2];
    // SAFETY: capget with a v3 header fills two Data structs.
    let r = unsafe { libc::syscall(libc::SYS_capget, &mut h as *mut Header, d.as_mut_ptr()) };
    (r == 0).then_some(d)
}

fn set(d: &[Data; 2]) -> bool {
    let mut h = Header { version: VERSION_3, pid: 0 };
    // SAFETY: capset with a v3 header reads two Data structs.
    unsafe { libc::syscall(libc::SYS_capset, &mut h as *mut Header, d.as_ptr()) == 0 }
}

/// Is CAP_SYS_PTRACE in our permitted set (granted by `engram setup`)?
pub fn ptrace_permitted() -> bool {
    get().is_some_and(|d| d[0].permitted & (1 << CAP_SYS_PTRACE) != 0)
}

fn set_effective(on: bool) {
    let Some(mut d) = get() else { return };
    let bit = 1 << CAP_SYS_PTRACE;
    if d[0].permitted & bit == 0 || (d[0].effective & bit != 0) == on {
        return;
    }
    if on {
        d[0].effective |= bit;
    } else {
        d[0].effective &= !bit;
    }
    set(&d);
}

/// Drop CAP_SYS_PTRACE from this thread's effective set (it stays permitted).
pub fn lower() {
    set_effective(false);
}

/// Run `f` with CAP_SYS_PTRACE effective on this thread, if we have it at all.
pub fn with_ptrace<T>(f: impl FnOnce() -> T) -> T {
    set_effective(true);
    let r = f();
    set_effective(false);
    r
}
