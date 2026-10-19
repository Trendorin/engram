# Engram — guide for coding agents

Engram is a memory scanner / reverse-engineering tool for Linux (native, Wine and Proton
programs), in the spirit of PINCE and Cheat Engine, written in Rust. Its distinguishing
feature: AI coding agents (OpenCode, Claude Code, ...) are first-class clients. A user can open
a terminal agent next to the window and say "find how many arrows I have and double it"; the
agent drives the same session the user sees.

Intended use: debugging and analysing the user's own programs and single-player games on their
own machine. Engram only attaches to processes of the same user.

Full documentation: `docs/wiki/en/` (Russian: `docs/wiki/ru/`). The architecture, how to add a
tool or a setting, testing and releases are in **docs/wiki/en/development.md** — read it
before larger changes.

## Layout

```
crates/
  engram-core/   process list, /proc maps, memory I/O, scanner, pointer scan,
                 disassembler (iced-x86), analysis helpers, hardware watchpoints
  engram-proto/  tools/ (the tool catalog, one file per group — single source of truth),
                 config (settings), blocking client, XDG paths, agent workspace kit
                 (assets/AGENTS.md is the playbook agents get, not this file)
  engramd/       engine daemon: daemon (state, events, approvals), target, table,
                 tools/ (handlers, one file per catalog group)
  engram-cli/    `engram`: routing in main, help, config, setup/doctor, uninstall, mcp, term
  engram-gui/    `engram-gui`: Slint window (ui/*.slint) with Qt backend + native style
tools/testbed/   tiny fake game with a known struct, for manual checks
packaging/       icon, .desktop template, package.sh (release archive)
install.sh       interactive installer / updater / uninstaller (wraps `engram uninstall`)
docs/wiki/       user and developer documentation (en, ru); en/tools.md is generated
```

## Commands

```sh
cargo build                                             # debug
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo run -p engram -- tools --markdown > docs/wiki/en/tools.md   # after changing tools
```

CI runs all of these and fails if `docs/wiki/en/tools.md` is stale. The first Slint + Qt build
takes about 10 minutes.

## Conventions

- Adding a tool: `ToolDef` in `engram-proto/src/tools/<group>.rs`, handler in
  `engramd/src/tools/<group>.rs`. `every_tool_is_routed` checks the pairing. Mutating tools
  call `d.approve(client, ...)` first.
- Adding a setting: `engram-proto/src/config.rs` (`Config`, `KEYS`, `get`, `set`), then
  `ui/settings.slint` + `src/settings.rs` in the GUI.
- Errors: `RpcError { code, message, hint }` with stable snake_case codes; the hint tells an
  agent what to do next.
- Output: addresses as hex strings, values as strings formatted per type.
- Params are lenient (`engramd/src/params.rs`): numbers may arrive as strings.
- GUI: `ui/state.slint` holds the `App` global (Rust writes properties, implements callbacks);
  never block the UI thread on the daemon — use the `quick` / `ops` workers in `ctx.rs`.
- Small modules, a doc comment at the top of each file, `// SAFETY:` on every unsafe block,
  no heavy dependencies (no tokio).
- Documentation changes go into both `docs/wiki/en` and `docs/wiki/ru`; user-visible changes
  into `CHANGELOG.md`.
- Do not commit unless the user asks.

## Testing by hand — never against the user's real setup

- Engine/CLI: isolated dirs, e.g. `XDG_RUNTIME_DIR=/run/user/$UID/engram-dev
  XDG_DATA_HOME=/tmp/engram-dev/data XDG_STATE_HOME=/tmp/engram-dev/state` (socket path under
  108 characters). Wrap CLI calls in `timeout`. With `ptrace_scope=1` start the testbed via
  `engram launch -- target/debug/engram-testbed <ctl-file>`.
- Installer and `engram uninstall`: only with a scratch `HOME` (keep `CARGO_HOME` and
  `RUSTUP_HOME` on the real toolchain) and an isolated `XDG_RUNTIME_DIR`. The user may have a
  real install in `~/.local` and a running engine; an unisolated test would replace or stop it.
- Window: with a changed `XDG_RUNTIME_DIR`, set `WAYLAND_DISPLAY=/run/user/$UID/wayland-1`.
  `ENGRAM_GUI_SNAPSHOT=<dir>` saves `snap-NNN.ppm` images of the window every 1.5 s.
- Stop processes by exact name (`pkill -x engramd`), never `pkill -f`.
