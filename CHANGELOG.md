# Changelog

All notable changes to Engram. The format follows [Keep a Changelog](https://keepachangelog.com/),
and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] — 2026-10-19

First release.

### Engine and tools
- Process list with Wine/Proton awareness: Windows names, Steam AppID, runtime helpers hidden.
- Value scans: exact, unknown, greater, less, between, not equal, changed, unchanged,
  increased/decreased (by); `auto` type across i32/i64/f32/f64 with ranking; byte patterns
  with wildcards; UTF-8 and UTF-16 text; one level of undo; cancellable with progress.
- Address table saved per game, freezing, address expressions with module offsets and
  pointer paths.
- `write` with an `expected` guard and read-back verification.
- Analysis: `analyze`, `dissect`, `disassemble` (x86/x64), `hexdump`, `sample`.
- Pointer scan with verified paths; hardware watchpoints (`watch`) for writes and accesses.
- `launch` to start programs as children of the engine (works without extra privileges).

### AI agents
- MCP server (`engram mcp`) with 27 tools; CLI with compact JSON for agents without MCP.
- `engram agent-init`: AGENTS.md playbook plus OpenCode and Claude Code configuration.
- Approval gate for anything that changes the target, with policies ask / allow / deny,
  "allow for session" and a configurable timeout.
- Live activity feed of every agent call; `notify` messages to the user.

### Window
- Slint window with the native Qt style, light and dark.
- Process picker with filter and "start a program", scan panel with live results and type
  filter, address table with freeze and inline editor, approval bar, Agent tab with
  "Open terminal", Inspect tab, Settings tab.
- Reconnects and restarts the engine automatically.

### Installation and settings
- Interactive installer (`install.sh`) for source checkouts and release archives, with
  `--yes`, `--prefix`, `--no-gui`, `--setup`, `--uninstall` and `--purge`.
- `engram setup` grants `CAP_SYS_PTRACE` to the engine only; `engram doctor` checks the setup.
- Settings in `~/.config/engram/config.json`, editable with `engram config` and in the window.
- `engram uninstall` removes exactly the installed files (manifest), optionally user data.

[Unreleased]: https://github.com/Trendorin/engram/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Trendorin/engram/releases/tag/v0.1.0
