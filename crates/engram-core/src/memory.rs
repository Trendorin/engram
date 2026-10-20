//! Remote memory I/O through `/proc/<pid>/mem`.
//!
//! The file is opened once, at attach, and every read and write goes through
//! that descriptor. It is bound to the address space the process had when it
//! was opened: if the process later executes another program (a setuid one
//! included), the descriptor stops working instead of following it. Writes use
//! the kernel's forced access, so read-only code pages can be patched too.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;

/// Addresses closer than this are read as one span by `read_scatter`.
const SPAN_GAP: u64 = 512;
/// Upper bound for one span read by `read_scatter`.
const SPAN_MAX: u64 = 1 << 20;

pub struct ProcMem {
    pid: i32,
    file: File,
}

impl ProcMem {
    /// Open the memory of `pid`. The caller checks that the process is ours
    /// ([`crate::process::may_access`]) before *and* after this call.
    pub fn open(pid: i32) -> io::Result<Self> {
        let file = crate::caps::with_ptrace(|| OpenOptions::new().read(true).write(true).open(format!("/proc/{pid}/mem")))?;
        Ok(Self { pid, file })
    }

    pub fn pid(&self) -> i32 {
        self.pid
    }

    /// Read as much as possible starting at `addr`. Stops at the first fault.
    pub fn read(&self, addr: u64, buf: &mut [u8]) -> io::Result<usize> {
        let mut done = 0;
        while done < buf.len() {
            match self.file.read_at(&mut buf[done..], addr.wrapping_add(done as u64)) {
                Ok(0) => break,
                Ok(n) => done += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                // EIO: the first page is not mapped. EINVAL: outside the address space.
                Err(e) if matches!(e.raw_os_error(), Some(libc::EIO | libc::EFAULT | libc::EINVAL)) => break,
                Err(e) if done == 0 => return Err(e),
                Err(_) => break,
            }
        }
        Ok(done)
    }

    pub fn read_exact(&self, addr: u64, buf: &mut [u8]) -> crate::Result<()> {
        let n = self.read(addr, buf)?;
        if n < buf.len() {
            return Err(crate::Error::new("unreadable", format!("{} is not readable ({} of {} bytes)", crate::hex(addr), n, buf.len())));
        }
        Ok(())
    }

    pub fn read_vec(&self, addr: u64, len: usize) -> crate::Result<Vec<u8>> {
        let mut v = vec![0u8; len];
        self.read_exact(addr, &mut v)?;
        Ok(v)
    }

    pub fn read_u64(&self, addr: u64) -> Option<u64> {
        let mut b = [0u8; 8];
        self.read_exact(addr, &mut b).ok()?;
        Some(u64::from_le_bytes(b))
    }

    pub fn read_ptr(&self, addr: u64, ptr_size: usize) -> Option<u64> {
        if ptr_size == 4 {
            let mut b = [0u8; 4];
            self.read_exact(addr, &mut b).ok()?;
            Some(u32::from_le_bytes(b) as u64)
        } else {
            self.read_u64(addr)
        }
    }

    /// Read `size` bytes at each address into `out` (`addrs.len() * size` bytes).
    /// Returns per-address success. Neighbouring addresses (scan results are
    /// sorted) are read as one span, so a next scan costs one call per cluster.
    pub fn read_scatter(&self, addrs: &[u64], size: usize, out: &mut [u8]) -> Vec<bool> {
        assert_eq!(out.len(), addrs.len() * size);
        let mut ok = vec![false; addrs.len()];
        let mut buf = Vec::new();
        let mut i = 0;
        while i < addrs.len() {
            let start = addrs[i];
            let Some(mut end) = start.checked_add(size as u64) else {
                i += 1;
                continue;
            };
            let mut j = i + 1;
            while j < addrs.len() {
                let a = addrs[j];
                match a.checked_add(size as u64) {
                    Some(e) if a >= start && a <= end.saturating_add(SPAN_GAP) && e - start <= SPAN_MAX => end = end.max(e),
                    _ => break,
                }
                j += 1;
            }
            let len = (end - start) as usize;
            buf.resize(len, 0);
            let got = self.read(start, &mut buf[..len]).unwrap_or(0);
            // Everything inside the bytes we got is done; a fault inside the span
            // (an unmapped gap) restarts the span at the first address after it.
            let mut next = j;
            for k in i..j {
                let off = (addrs[k] - start) as usize;
                if off + size <= got {
                    out[k * size..(k + 1) * size].copy_from_slice(&buf[off..off + size]);
                    ok[k] = true;
                } else {
                    next = k;
                    break;
                }
            }
            i = if next == i { i + 1 } else { next };
        }
        ok
    }

    pub fn write(&self, addr: u64, data: &[u8]) -> crate::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        self.file.write_all_at(data, addr)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_self() {
        let mem = ProcMem::open(std::process::id() as i32).unwrap();
        let mut target: Box<[u32; 4]> = Box::new([11, 22, 33, 44]);
        let base = target.as_ptr() as u64;
        let mut buf = [0u8; 4];
        mem.read_exact(base + 4, &mut buf).unwrap();
        assert_eq!(u32::from_le_bytes(buf), 22);
        let addrs = [base, base + 8, 8];
        let mut out = vec![0u8; 12];
        let ok = mem.read_scatter(&addrs, 4, &mut out);
        assert_eq!(ok, vec![true, true, false]);
        assert_eq!(u32::from_le_bytes(out[4..8].try_into().unwrap()), 33);
        mem.write(base + 12, &99u32.to_le_bytes()).unwrap();
        assert_eq!(std::hint::black_box(&mut target)[3], 99);
    }

    #[test]
    fn scatter_across_a_hole() {
        // Two pages with an unmapped page between them: the span read stops at the
        // hole and resumes after it.
        let page = 4096usize;
        // SAFETY: anonymous mapping of three pages; the middle one is unmapped below.
        let p = unsafe {
            libc::mmap(std::ptr::null_mut(), 3 * page, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS, -1, 0)
        };
        assert_ne!(p, libc::MAP_FAILED);
        let base = p as u64;
        unsafe {
            *(p as *mut u32).add(1) = 7;
            *((p as usize + 2 * page) as *mut u32) = 9;
            libc::munmap((p as usize + page) as *mut _, page);
        }
        let mem = ProcMem::open(std::process::id() as i32).unwrap();
        let addrs = [base + 4, base + page as u64 + 8, base + 2 * page as u64];
        let mut out = vec![0u8; 12];
        assert_eq!(mem.read_scatter(&addrs, 4, &mut out), vec![true, false, true]);
        assert_eq!(u32::from_le_bytes(out[0..4].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(out[8..12].try_into().unwrap()), 9);
        unsafe {
            libc::munmap(p, page);
            libc::munmap((p as usize + 2 * page) as *mut _, page);
        }
    }
}
