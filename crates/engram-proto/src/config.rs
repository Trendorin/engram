//! User settings in `$XDG_CONFIG_HOME/engram/config.json`.
//!
//! One table (`KEYS`) describes every setting: the CLI (`engram config`), the
//! GUI settings tab and the docs all read it. Unknown or invalid values in the
//! file fall back to defaults, so a hand-edited file can never stop Engram.

use crate::paths;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Approval policy the engine starts with: ask | allow | deny.
    pub policy: String,
    /// Seconds an agent's request waits for the user before it is refused.
    pub approval_timeout_s: u64,
    /// How often frozen values are rewritten.
    pub freeze_interval_ms: u64,
    /// Agent preselected for "Open terminal" (label, e.g. "OpenCode").
    pub agent: String,
    /// Terminal emulator command; empty = detect.
    pub terminal: String,
    /// Agent workspace folder; empty = `~/.local/share/engram/workspace`.
    pub workspace: String,
    /// Scan results shown in the window.
    pub result_rows: u64,
    /// Refresh interval of live values in the window.
    pub refresh_ms: u64,
    /// Language of the window: auto (follow the system) | en | ru.
    pub language: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            policy: "ask".into(),
            approval_timeout_s: 180,
            freeze_interval_ms: 100,
            agent: String::new(),
            terminal: String::new(),
            workspace: String::new(),
            result_rows: 1000,
            refresh_ms: 600,
            language: "auto".into(),
        }
    }
}

pub struct Key {
    pub name: &'static str,
    pub description: &'static str,
    /// Accepted values, for help output.
    pub accepts: &'static str,
}

pub const KEYS: &[Key] = &[
    Key { name: "policy", description: "Agent memory writes when the engine starts", accepts: "ask | allow | deny" },
    Key { name: "approval_timeout_s", description: "Seconds an agent request waits for your answer", accepts: "10..3600" },
    Key { name: "freeze_interval_ms", description: "How often frozen values are rewritten", accepts: "20..5000" },
    Key {
        name: "agent",
        description: "Agent preselected for \"Open terminal\"",
        accepts: "OpenCode, Claude Code, Codex, Gemini CLI, Shell only, or empty",
    },
    Key { name: "terminal", description: "Terminal emulator (empty = detect)", accepts: "a command such as kitty or konsole" },
    Key { name: "workspace", description: "Folder agents are started in (empty = default)", accepts: "a path; ~ is allowed" },
    Key { name: "result_rows", description: "Scan results shown in the window", accepts: "50..1000" },
    Key { name: "refresh_ms", description: "Refresh interval of live values in the window", accepts: "200..5000" },
    Key { name: "language", description: "Language of the window", accepts: "auto | en | ru" },
];

fn range(key: &str, v: &str, lo: u64, hi: u64) -> Result<u64, String> {
    let n: u64 = v.trim().parse().map_err(|_| format!("{key} must be a whole number"))?;
    if (lo..=hi).contains(&n) {
        Ok(n)
    } else {
        Err(format!("{key} must be between {lo} and {hi}"))
    }
}

impl Config {
    /// Settings from disk; defaults if the file is missing or unreadable.
    pub fn load() -> Self {
        std::fs::read_to_string(paths::config_file())
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).ok())
            .map(Config::sanitized)
            .unwrap_or_default()
    }

    /// Write atomically (temp file + rename).
    pub fn save(&self) -> io::Result<()> {
        let dir = paths::config_dir();
        std::fs::create_dir_all(&dir)?;
        let tmp = dir.join("config.json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)? + "\n")?;
        std::fs::rename(tmp, paths::config_file())
    }

    /// Replace out-of-range values with defaults.
    fn sanitized(self) -> Self {
        let d = Config::default();
        let mut c = Config::default();
        for k in KEYS {
            if c.set(k.name, &self.get(k.name).unwrap_or_default()).is_err() {
                let _ = c.set(k.name, &d.get(k.name).unwrap_or_default());
            }
        }
        c
    }

    pub fn get(&self, key: &str) -> Option<String> {
        Some(match key {
            "policy" => self.policy.clone(),
            "approval_timeout_s" => self.approval_timeout_s.to_string(),
            "freeze_interval_ms" => self.freeze_interval_ms.to_string(),
            "agent" => self.agent.clone(),
            "terminal" => self.terminal.clone(),
            "workspace" => self.workspace.clone(),
            "result_rows" => self.result_rows.to_string(),
            "refresh_ms" => self.refresh_ms.to_string(),
            "language" => self.language.clone(),
            _ => return None,
        })
    }

    /// Validate and set one value given as text.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let v = value.trim();
        match key {
            "policy" => match v.to_ascii_lowercase().as_str() {
                p @ ("ask" | "allow" | "deny") => self.policy = p.to_string(),
                _ => return Err("policy must be ask, allow or deny".into()),
            },
            "approval_timeout_s" => self.approval_timeout_s = range(key, v, 10, 3600)?,
            "freeze_interval_ms" => self.freeze_interval_ms = range(key, v, 20, 5000)?,
            "agent" => self.agent = v.to_string(),
            "terminal" => self.terminal = v.to_string(),
            "workspace" => self.workspace = v.to_string(),
            "result_rows" => self.result_rows = range(key, v, 50, 1000)?,
            "refresh_ms" => self.refresh_ms = range(key, v, 200, 5000)?,
            "language" => match v.to_ascii_lowercase().as_str() {
                l @ ("auto" | "en" | "ru") => self.language = l.to_string(),
                _ => return Err("language must be auto, en or ru".into()),
            },
            _ => {
                let names: Vec<&str> = KEYS.iter().map(|k| k.name).collect();
                return Err(format!("unknown setting '{key}' (one of: {})", names.join(", ")));
            }
        }
        Ok(())
    }

    /// Reset one key to its default.
    pub fn reset(&mut self, key: &str) -> Result<(), String> {
        let d = Config::default().get(key).ok_or_else(|| format!("unknown setting '{key}'"))?;
        self.set(key, &d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_validates_and_roundtrips() {
        let mut c = Config::default();
        assert!(c.set("policy", "ALLOW").is_ok());
        assert_eq!(c.policy, "allow");
        assert!(c.set("policy", "maybe").is_err());
        assert!(c.set("refresh_ms", "50").is_err());
        assert!(c.set("refresh_ms", "1000").is_ok());
        assert!(c.set("nope", "1").is_err());
        for k in KEYS {
            let v = c.get(k.name).expect("every key readable");
            assert!(c.clone().set(k.name, &v).is_ok(), "{} roundtrips", k.name);
        }
    }

    #[test]
    fn bad_file_values_fall_back() {
        let c: Config = serde_json::from_str(r#"{"policy":"weird","refresh_ms":1,"terminal":"kitty","extra":true}"#).unwrap();
        let c = c.sanitized();
        assert_eq!(c.policy, "ask");
        assert_eq!(c.refresh_ms, 600);
        assert_eq!(c.terminal, "kitty");
    }
}
