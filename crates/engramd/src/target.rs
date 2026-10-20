//! The attached process: identity, memory handle, cached layout and address
//! expression resolution.

use crate::daemon::core_err;
use engram_core::expr::{self, Resolver};
use engram_core::maps::{self, Module, Region};
use engram_core::memory::ProcMem;
use engram_core::process::{self, ProcessKind};
use engram_proto::RpcError;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Target {
    pub pid: i32,
    pub start_time: u64,
    pub name: String,
    pub kind: ProcessKind,
    pub steam_app_id: Option<String>,
    pub mem: ProcMem,
    pub ptr_size: usize,
    layout: Mutex<Layout>,
}

/// When the memory map was read, the regions and the modules derived from them.
type Layout = (Instant, Arc<Vec<Region>>, Arc<Vec<Module>>);

impl Target {
    pub fn open(info: &process::ProcessInfo) -> Result<Self, RpcError> {
        let forbidden = || {
            RpcError::new("forbidden", format!("{} is not an ordinary process of your user", info.name))
            .hint("Engram does not attach to setuid, setgid or privileged programs, or to ones that protect their memory (ssh-agent, gpg-agent)")
        };
        if !process::may_access(info.pid) {
            return Err(forbidden());
        }
        let mem = ProcMem::open(info.pid).map_err(|e| core_err(e.into()))?;
        // It may have executed a privileged program between the check and the open.
        if !process::may_access(info.pid) || !process::is_same(info.pid, info.start_time) {
            return Err(forbidden());
        }
        let regions = maps::read_maps(info.pid).map_err(|e| core_err(e.into()))?;
        let mods = maps::modules(&regions);
        // Probe access on the first readable region so permission problems surface at attach.
        if let Some(r) = regions.iter().find(|r| r.scannable()) {
            let mut b = [0u8; 1];
            mem.read(r.start, &mut b).map_err(|e| core_err(e.into()))?;
        }
        let ptr_size = engram_core::analyze::pointer_size(&mem, &mods, &info.name);
        Ok(Self {
            pid: info.pid,
            start_time: info.start_time,
            name: info.name.clone(),
            kind: info.kind,
            steam_app_id: info.steam_app_id.clone(),
            mem,
            ptr_size,
            layout: Mutex::new((Instant::now(), Arc::new(regions), Arc::new(mods))),
        })
    }

    /// Still the process we attached to, and still an ordinary one of ours.
    pub fn alive(&self) -> bool {
        process::is_same(self.pid, self.start_time) && process::may_access(self.pid)
    }

    /// Memory map + modules, cached for a second (maps change as games allocate).
    pub fn layout(&self) -> (Arc<Vec<Region>>, Arc<Vec<Module>>) {
        let mut g = self.layout.lock().unwrap();
        if g.0.elapsed() > Duration::from_millis(1000) {
            if let Ok(regions) = maps::read_maps(self.pid) {
                let mods = maps::modules(&regions);
                *g = (Instant::now(), Arc::new(regions), Arc::new(mods));
            }
        }
        (g.1.clone(), g.2.clone())
    }

    pub fn fresh_layout(&self) -> (Arc<Vec<Region>>, Arc<Vec<Module>>) {
        self.layout.lock().unwrap().0 = Instant::now() - Duration::from_secs(10);
        self.layout()
    }

    pub fn bitness(&self) -> u32 {
        if self.ptr_size == 4 {
            32
        } else {
            64
        }
    }

    pub fn resolve(&self, expression: &str) -> Result<u64, RpcError> {
        if let Some(a) = engram_core::parse_addr(expression) {
            return Ok(a);
        }
        let (_, mods) = self.layout();
        let r = TargetResolver { t: self, mods: &mods };
        expr::eval(expression, &r).map_err(core_err)
    }

    pub fn info(&self) -> Value {
        json!({
            "pid": self.pid,
            "name": self.name,
            "kind": self.kind,
            "pointer_size": self.ptr_size,
            "steam_app_id": self.steam_app_id,
        })
    }
}

struct TargetResolver<'a> {
    t: &'a Target,
    mods: &'a [Module],
}

impl Resolver for TargetResolver<'_> {
    fn module_base(&self, name: &str) -> Option<u64> {
        maps::find_module(self.mods, name).map(|m| m.base)
    }
    fn read_ptr(&self, addr: u64) -> Option<u64> {
        self.t.mem.read_ptr(addr, self.t.ptr_size)
    }
}
