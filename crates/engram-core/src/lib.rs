//! Engram core: everything that touches a target process.
//!
//! The crate is deliberately free of any IPC or UI concerns. `engramd` owns the
//! session state and exposes these primitives to the GUI and to AI agents.

pub mod analyze;
pub mod caps;
pub mod disasm;
pub mod expr;
pub mod maps;
pub mod memory;
pub mod pointer;
pub mod process;
pub mod scan;
pub mod value;
pub mod watch;

use std::fmt;

/// Error with a stable machine-readable code (agents branch on `code`).
#[derive(Debug, Clone)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}

impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_argument", message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        let code = match e.raw_os_error() {
            Some(libc::EPERM) | Some(libc::EACCES) => "permission_denied",
            Some(libc::ESRCH) => "process_gone",
            Some(libc::EFAULT) | Some(libc::EIO) => "unreadable",
            _ => "io_error",
        };
        Error::new(code, e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Format an address the same way everywhere.
pub fn hex(addr: u64) -> String {
    format!("0x{addr:x}")
}

/// Parse "0x1f", "1f" (hex, the memory-tool convention) or "#31" (decimal).
pub fn parse_addr(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(d) = s.strip_prefix('#') {
        return d.parse().ok();
    }
    let h = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(&h.replace('_', ""), 16).ok()
}
