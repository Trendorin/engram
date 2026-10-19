//! The address table ("cheat table"): named entries, per-game persistence and
//! the freezer thread that keeps frozen values in place.

use crate::daemon::Daemon;
use engram_core::value::DataType;
use engram_proto::paths;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: u32,
    pub description: String,
    /// Address expression (absolute hex or module/pointer path).
    pub address: String,
    #[serde(rename = "type")]
    pub dtype: DataType,
    /// Bytes rewritten while frozen. Never persisted: freezes are session-only.
    #[serde(skip)]
    pub frozen: Option<Vec<u8>>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Table {
    pub entries: Vec<Entry>,
    pub next_id: u32,
    #[serde(skip)]
    pub game: Option<String>,
}

fn file_for(game: &str) -> std::path::PathBuf {
    let safe: String = game
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c.to_ascii_lowercase() } else { '_' })
        .collect();
    paths::tables_dir().join(format!("{safe}.json"))
}

impl Table {
    pub fn add(&mut self, description: String, address: String, dtype: DataType) -> u32 {
        self.next_id = self.next_id.max(self.entries.iter().map(|e| e.id).max().unwrap_or(0)) + 1;
        let id = self.next_id;
        self.entries.push(Entry { id, description, address, dtype, frozen: None });
        self.save();
        id
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    pub fn unfreeze_all(&mut self) {
        self.entries.iter_mut().for_each(|e| e.frozen = None);
    }

    pub fn save(&self) {
        let Some(game) = &self.game else { return };
        let _ = std::fs::create_dir_all(paths::tables_dir());
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(file_for(game), s);
        }
    }

    /// Switch to `game`'s table (saving the current one). Returns entries loaded.
    pub fn switch_game(&mut self, game: &str) -> usize {
        if self.game.as_deref() == Some(game) {
            return self.entries.len();
        }
        self.save();
        let loaded: Table = std::fs::read_to_string(file_for(game)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        *self = loaded;
        self.game = Some(game.to_string());
        self.entries.len()
    }
}

/// Rewrites frozen entries (every `freeze_interval_ms`) and notices when the target exits.
pub fn spawn_freezer(d: Arc<Daemon>) {
    std::thread::spawn(move || {
        let mut last_alive_check = Instant::now();
        loop {
            let interval = d.config.read().unwrap().freeze_interval_ms;
            std::thread::sleep(Duration::from_millis(interval));
            let target = d.target.read().unwrap().clone();
            let Some(t) = target else { continue };
            if last_alive_check.elapsed() > Duration::from_millis(700) {
                last_alive_check = Instant::now();
                if !t.alive() {
                    d.drop_target("exited");
                    continue;
                }
            }
            let frozen: Vec<(String, Vec<u8>)> =
                d.table.lock().unwrap().entries.iter().filter_map(|e| Some((e.address.clone(), e.frozen.clone()?))).collect();
            for (expr, bytes) in frozen {
                if let Ok(addr) = t.resolve(&expr) {
                    let _ = t.mem.write(addr, &bytes);
                }
            }
        }
    });
}
