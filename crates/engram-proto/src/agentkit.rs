//! Prepare a folder so any coding agent (OpenCode, Claude Code, Codex, ...)
//! started in it knows how to drive Engram: AGENTS.md playbook plus MCP config.

use serde_json::{json, Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const TEMPLATE: &str = include_str!("../assets/AGENTS.md");
const BEGIN: &str = "<!-- engram:begin -->";
const END: &str = "<!-- engram:end -->";

pub fn agents_md(engram_bin: &Path) -> String {
    TEMPLATE.replace("{{ENGRAM}}", &engram_bin.display().to_string())
}

/// Short instructions for MCP `initialize`.
pub const MCP_INSTRUCTIONS: &str = "Engram reads and edits memory of the user's running programs (games). \
Call `status` first. Game values change only when the user acts: ask them to change a value in-game, \
then call scan_next. Writes need the user's approval in the Engram window; always pass `expected` and \
`reason`. Save finds with table_add. Use pointer_scan for addresses that survive restarts.";

fn upsert_block(path: &Path, block: &str) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let new = match (existing.find(BEGIN), existing.find(END)) {
        (Some(b), Some(e)) if e > b => format!("{}{}{}", &existing[..b], block.trim_end(), &existing[e + END.len()..]),
        _ if existing.trim().is_empty() => block.to_string(),
        _ => format!("{}\n\n{}", existing.trim_end(), block),
    };
    fs::write(path, new)
}

fn merge_json(path: &Path, pointer: &[&str], value: Value, base: Value) -> io::Result<()> {
    // A file that exists but is not valid JSON is the user's: report it instead of overwriting it.
    let mut root: Value = match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("{} is not valid JSON ({e}); fix or remove it", path.display()))
        })?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => base,
        Err(e) => return Err(e),
    };
    let mut cur = &mut root;
    for key in pointer {
        if !cur.is_object() {
            *cur = Value::Object(Map::new());
        }
        cur = cur.as_object_mut().unwrap().entry(key.to_string()).or_insert_with(|| Value::Object(Map::new()));
    }
    *cur = value;
    fs::write(path, serde_json::to_string_pretty(&root)? + "\n")
}

/// Write/merge AGENTS.md, CLAUDE.md, opencode.json and .mcp.json into `dir`.
/// Existing files are merged, never clobbered.
pub fn init_workspace(dir: &Path, engram_bin: &Path) -> io::Result<Vec<PathBuf>> {
    fs::create_dir_all(dir)?;
    let bin = engram_bin.display().to_string();
    let agents = dir.join("AGENTS.md");
    upsert_block(&agents, &agents_md(engram_bin))?;

    let claude = dir.join("CLAUDE.md");
    let claude_text = fs::read_to_string(&claude).unwrap_or_default();
    if !claude_text.contains("@AGENTS.md") {
        fs::write(&claude, format!("{}@AGENTS.md\n", if claude_text.is_empty() { String::new() } else { claude_text + "\n" }))?;
    }

    let opencode = dir.join("opencode.json");
    merge_json(
        &opencode,
        &["mcp", "engram"],
        json!({ "type": "local", "command": [bin, "mcp"], "enabled": true }),
        json!({ "$schema": "https://opencode.ai/config.json" }),
    )?;

    let mcp = dir.join(".mcp.json");
    merge_json(&mcp, &["mcpServers", "engram"], json!({ "command": bin, "args": ["mcp"] }), json!({}))?;

    Ok(vec![agents, claude, opencode, mcp])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent_and_merges() {
        let dir = std::env::temp_dir().join(format!("engram-kit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("AGENTS.md"), "# My project\n").unwrap();
        fs::write(dir.join("opencode.json"), r#"{"model":"x","mcp":{"other":{"type":"local"}}}"#).unwrap();
        let bin = Path::new("/opt/engram/engram");
        init_workspace(&dir, bin).unwrap();
        init_workspace(&dir, bin).unwrap();
        let agents = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert!(agents.starts_with("# My project"));
        assert_eq!(agents.matches(BEGIN).count(), 1);
        assert!(agents.contains("/opt/engram/engram status"));
        let oc: Value = serde_json::from_str(&fs::read_to_string(dir.join("opencode.json")).unwrap()).unwrap();
        assert_eq!(oc["model"], "x");
        assert!(oc["mcp"]["other"].is_object());
        assert_eq!(oc["mcp"]["engram"]["command"][1], "mcp");
        let _ = fs::remove_dir_all(&dir);
    }
}
