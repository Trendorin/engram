# Development

## Architecture

```
 engram-gui ─┐
 engram CLI ─┼── Unix socket, newline-delimited JSON ──► engramd ──► /proc/<pid>/mem, ptrace
 engram mcp ─┘   ($XDG_RUNTIME_DIR/engram, 0600, uid-checked)
     ▲
     └── OpenCode / Claude Code / Codex / Gemini (MCP or CLI)
```

`engramd` owns the single session: target, scan with one undo level, address table, freezes,
approval policy and the activity log. Every client is equal and can auto-start the daemon.

- Requests: `{"id":1,"method":"scan","params":{...},"client":"mcp:opencode"}`
- Replies: `{"id":1,"ok":true,"result":{...}}` or `{"id":1,"ok":false,"error":{"code","message","hint"}}`
- A connection that sends `subscribe` turns into an event stream: `activity`, `target`,
  `scan`, `table`, `progress`, `notify`, `approval_request`, `approval_resolved`, `policy`,
  `memory`, `launch`.
- Tools with `Access::Mutate` are gated. Calls from non-window clients wait for
  `approval_respond` from the window, or follow the policy.

## Crates

| Crate | Modules |
|---|---|
| `engram-core` | `process` (Wine/Proton aware list), `maps`, `memory` (`/proc/pid/mem`, `process_vm_readv`), `scan`, `pointer`, `expr`, `value`, `analyze`, `disasm` (iced-x86), `watch` (debug registers) |
| `engram-proto` | `tools/` (the catalog, one file per group), `config` (settings), `client`, `paths`, `agentkit` (workspace + AGENTS.md template) |
| `engramd` | `daemon` (state, events, approvals), `target`, `table` (+ freezer), `tools/` (handlers, one file per group), `params` |
| `engram-cli` | `main` (routing), `help`, `config`, `setup` (doctor/setup/agent-init), `uninstall`, `mcp`, `term` |
| `engram-gui` | `ui/*.slint` (views; `state.slint` = the `App` global), `ctx`, `events`, `wire`, `view`, `poll`, `inspect`, `settings`, `launcher`, `engine`, `format` |

## Adding a tool

1. Add a `ToolDef` to the right group in `crates/engram-proto/src/tools/<group>.rs`. Write
   the description for a language model: what it does, when to use it, what to do next. Choose
   `Access::Mutate` if it changes the target.
2. Handle it in `crates/engramd/src/tools/<group>.rs` (`dispatch` + a function). Call
   `d.approve(client, ...)` before changing anything if it is `Mutate`.
3. Return errors as `RpcError::new("snake_case_code", msg).hint("what to do next")`.
4. Regenerate the reference: `cargo run -p engram -- tools --markdown > docs/wiki/en/tools.md`.
5. If it changes how agents should work, update `crates/engram-proto/assets/AGENTS.md`.

The CLI help and the MCP tool list pick the tool up automatically. The test
`every_tool_is_routed` fails if a catalog tool has no handler.

New groups go into `GROUPS` in `engram-proto/src/tools/mod.rs` and into the dispatch chain in
`engramd/src/tools/mod.rs`.

## Adding a setting

Add a field with a default to `Config` in `crates/engram-proto/src/config.rs`, an entry in
`KEYS`, and arms in `get` and `set` (with validation). `engram config` shows it automatically.
For the window, add a field to `ui/settings.slint` and `src/settings.rs`.

## Building, testing, linting

```sh
cargo build                      # debug
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

`.cargo/config.toml` points Slint to `/usr/bin/qmake6`. Override it with `QMAKE=...`. Slint
1.18 needs the `backend-qt` feature for the Qt backend; `build.rs` picks the `native` style
(`qt` if Qt was found, otherwise `fluent`). `SLINT_STYLE` overrides the style.

## Manual end-to-end checks

Keep tests away from your real session and real installation: use a separate runtime and data
location, or a scratch `HOME`.

```sh
export XDG_RUNTIME_DIR=/run/user/$UID/engram-dev XDG_DATA_HOME=/tmp/engram-dev/data XDG_STATE_HOME=/tmp/engram-dev/state
mkdir -p -m700 $XDG_RUNTIME_DIR
cargo build
target/debug/engram launch -- target/debug/engram-testbed /tmp/engram-dev/ctl &
target/debug/engram attach name=engram-testbed
target/debug/engram scan value=37
echo loot > /tmp/engram-dev/ctl          # arrows 37 → 36, gold +25
target/debug/engram scan_next value=36
```

The testbed (`tools/testbed`) is a tiny fake game with a known struct. It accepts the commands
`hit`, `loot`, `move` and `quit` through its control file.

For the window: with a changed `XDG_RUNTIME_DIR`, point Wayland at the real display socket
(`WAYLAND_DISPLAY=/run/user/$UID/wayland-1`). `ENGRAM_GUI_SNAPSHOT=<dir>` makes the window save
`snap-NNN.ppm` images every 1.5 s. Drive the session through the CLI with
`ENGRAM_CLIENT=<agent>` to see agent activity and approvals. Stop processes by exact name
(`pkill -x engramd`).

Test the installer only with a scratch `HOME` (keep `CARGO_HOME` and `RUSTUP_HOME` pointing at
your real toolchain). It installs into, and uninstalls from, `$HOME/.local`.

## Releases

1. Update the version in `Cargo.toml` (`[workspace.package]`) and `CHANGELOG.md`.
2. Commit, tag `vX.Y.Z` and push the tag.
3. The `release` workflow builds on Ubuntu 24.04, runs `packaging/package.sh` and publishes
   `engram-X.Y.Z-x86_64-linux.tar.gz` with its `.sha256` on a GitHub release.

`packaging/package.sh` also works locally and writes to `dist/`.

## Known limits and ideas

- Linux x86-64 only (the watchpoints and disassembler are x86-specific).
- Ideas: a structure editor on top of `dissect`, a memory viewer pane, importing Cheat Engine
  `.CT` tables, an AUR package.
