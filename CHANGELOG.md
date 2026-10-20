# Changelog

All notable changes to Engram. The format follows [Keep a Changelog](https://keepachangelog.com/),
and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.1.1] — 2026-10-20

### Fixed
- The AppImage did not start on Wayland: it lacked Qt's Wayland shell plugins (the build tool that packs it
  was pinned to a release that does not deploy them). The release workflow now checks that they are inside.
  The distribution packages were not affected.


## [1.1.0] — 2026-10-20

### Added
- **Language of the window** (Settings → Window and engine → Language): follow the system, English or
  Russian. It changes at once when saved; the same setting is `engram config set language ru`. The
  whole window is translated, including the messages it shows. The command line, the MCP tools and the
  texts agents see stay English.


## [1.0.1] — 2026-10-20

Security and correctness fixes after an independent review.

### Security
- `engram setup` no longer trusts the path of the engine: the elevated step opens the file once,
  checks that it is the content that was hashed before the password prompt, and applies
  `CAP_SYS_PTRACE` to that open file, so swapping the binary while the prompt is open grants nothing.
- The GUI's and the launcher's exemption from approvals is decided by the kernel's view of the
  connecting process, not by a name in the request: `ENGRAM_CLIENT=gui engram write …` is an
  ordinary client and is asked like any other.
- Clients verify with `SO_PEERCRED` that the engine socket belongs to their own user before sending
  anything (the launcher sends its environment), so a socket another user put in `/tmp` is refused.
- Editing the address or type of a frozen entry ends the freeze instead of writing the old bytes
  to the new address without approval; freezes do not carry over to another process of the same game.
- `write` checks `expected` again after the approval, so a value that changed while the prompt was
  open is not overwritten.
- The installer no longer interpolates the prefix into `sed` programs (a prefix could run commands),
  quotes the path in the desktop entry, and the uninstaller refuses manifest entries that leave the
  prefix through `..` or symlinks.
- Release builds pin every GitHub Action to a commit and check the AppImage tools against a SHA-256.

### Fixed
- Address expressions nested very deeply are an error instead of a stack overflow that stopped the engine.
- Hex input and module names with non-ASCII characters no longer panic; a pointer value that does not fit
  the target's pointer size is rejected instead of being truncated.
- Exact byte and string scans find overlapping matches (`aa` in `aaa`).
- A scan that finishes after another process was attached is discarded instead of replacing its session.
- Editing the value of a frozen entry keeps the new value instead of being undone by the freezer.
- `engram scan value=1.000` keeps the three decimals (they set the tolerance); `--compact` and
  `--pretty` after `--` belong to the launched program.
- `engram agent-init` stops with an error when `.mcp.json` or `opencode.json` is not valid JSON instead of
  overwriting it, and `engram uninstall` works through a symlinked prefix.


## [1.0.0] — 2026-10-20

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

### Security
- Engram attaches only to ordinary processes of your own user, by the same rules the kernel
  uses for ptrace, without the bypass that `CAP_SYS_PTRACE` would give: every user and group id
  of the target must be yours, it may hold no capability the engine lacks, and it must be
  dumpable. Setuid, setgid and file-capability programs (sudo, passwd) and processes that
  protect their memory (ssh-agent, gpg-agent) are refused and not listed.
- Memory is read and written through a `/proc/<pid>/mem` descriptor opened at attach and
  checked again afterwards. If the target executes another program, the descriptor stops
  working instead of following it; hardware watchpoints detach before the new program runs.
- The engine keeps `CAP_SYS_PTRACE` out of its effective set except around attaching.

### Installation and settings
- Packages for Arch Linux, Debian and Fedora, and an AppImage. Each one is built in a clean
  container of its distribution, installed and started there before a release.
- Interactive installer (`install.sh`) for source checkouts, with `--yes`, `--prefix`,
  `--no-gui`, `--setup`, `--uninstall` and `--purge`.
- `engram setup` grants `CAP_SYS_PTRACE` to the engine only; `engram doctor` checks the setup.
- Settings in `~/.config/engram/config.json`, editable with `engram config` and in the window.
- `engram uninstall` removes exactly the installed files (manifest), optionally user data.

[Unreleased]: https://github.com/Trendorin/engram/compare/v1.1.1...HEAD
[1.1.1]: https://github.com/Trendorin/engram/releases/tag/v1.1.1
[1.1.0]: https://github.com/Trendorin/engram/releases/tag/v1.1.0
[1.0.1]: https://github.com/Trendorin/engram/releases/tag/v1.0.1
[1.0.0]: https://github.com/Trendorin/engram/releases/tag/v1.0.0
