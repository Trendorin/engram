<div align="center">

<img src="packaging/engram.svg" width="88" alt="">

# Engram

**A memory scanner and debugger for Linux that you can drive with an AI agent.**

[Install](#install) · [Wiki](docs/wiki/en/README.md) · [Tool reference](docs/wiki/en/tools.md) · [Русский](README.ru.md)

</div>

Engram works like Cheat Engine or PINCE. You can find the number of arrows, gold or health in
a running program, change it, freeze it, find the code that writes it and save a pointer path
that survives restarts. It works with native Linux programs and Windows games under Wine and
Proton.

AI coding agents (OpenCode, Claude Code, Codex, Gemini CLI) are full clients. Open a terminal
next to the window and say *"find how many arrows I have and double it"*. The agent works in
the same session you see, asks you to pick something up in the game, narrows the scan and saves
the address to your table. Every memory write waits until you click **Allow**.

![Engram: an agent's write waiting for approval, scan results, the address table and the live activity feed](docs/images/screenshot.png)

> Engram is for your own programs and single-player games on your own machine. It only
> attaches to processes of your own user. Online games with anti-cheat may ban you for this.

## Features

- **Value scans**: exact, unknown, greater/less/between, changed/unchanged,
  increased/decreased (by). The `auto` type scans i32, i64, f32 and f64 in one pass and ranks
  the hits. Byte patterns with `??` wildcards and UTF-8/UTF-16 text are supported, with one
  step of undo.
- **Address table**: saved per game and reloaded on attach. Freeze and edit entries, and use
  expressions such as `Game.exe+1A2B30` or `[[Game.exe+10]+18]+40`.
- **Analysis**: explain an address (region, module+offset, every type, pointer target), guess
  structures, disassemble x86/x64, hex dump, and sample values over time.
- **Pointer scan**: verified module-based paths that survive restarts.
- **Find what writes or accesses** an address with hardware watchpoints on every thread.
- **Wine/Proton aware**: games appear as `Game.exe`, `.exe`/`.dll` images are modules, and
  32-bit games are handled automatically.
- **Agents built in**: an MCP server, a CLI with compact JSON output, an `AGENTS.md` playbook
  and one-click "Open terminal". Writes need your approval, set per policy.
- **Native look**: the Slint window uses your Qt style and colour scheme.
- **Clean install and removal**: an interactive installer, `engram config` for settings and
  `engram uninstall` that removes exactly what was installed.

## Install

**Release archive** (no Rust needed): download `engram-<version>-x86_64-linux.tar.gz` from
[Releases](https://github.com/trendorion/engram/releases), unpack it and run `./install.sh`.

**From source:** you need Rust (stable), a C++ compiler and Qt 6 development files
(`qt6-base` on Arch, `qt6-base-dev` on Debian/Ubuntu, `qt6-qtbase-devel` on Fedora).

```sh
git clone https://github.com/trendorion/engram
cd engram
./install.sh
```

The installer asks a few questions (location, window, memory access), builds, installs into
`~/.local` and adds a menu entry. Then:

```sh
engram setup     # once: let Engram read your games (asks for your password)
engram doctor    # check everything
```

Most distributions only let a program read the memory of programs it started.
`engram setup` grants one capability (`CAP_SYS_PTRACE`) to the small engine binary only.
Without it, start games through Engram: use the window's "Start a program" field, or set
`engram launch -- %command%` as the Steam launch option. See [Installation](docs/wiki/en/install.md).

## Quick start

1. Open **Engram** from your menu (`engram-gui`) and click **Select process…**.
2. Type the value the game shows and press **First scan**.
3. Change the value in the game, type the new value and press **Next scan**. Repeat.
4. Double-click a result to save it. Then edit it, tick **Freeze**, or use **Inspect**.

With an agent: in the **Agent** tab press **Open terminal**, then ask for what you want.
To set it up by hand:

```sh
claude mcp add engram -- engram mcp         # Claude Code
codex mcp add engram -- engram mcp          # Codex
engram agent-init ~/some-folder             # AGENTS.md + opencode.json + .mcp.json
```

From a shell:

```sh
engram attach name=Game.exe
engram scan value=37
engram scan_next value=36
engram table_add address=0x7f12a0 type=i32 description=Arrows
```

## Documentation

| | |
|---|---|
| [Installation](docs/wiki/en/install.md) | installer options, memory access, updating, uninstalling |
| [The window](docs/wiki/en/window.md) | every panel and button |
| [AI agents](docs/wiki/en/agents.md) | MCP setup, approvals and policy |
| [Concepts](docs/wiki/en/concepts.md) | address expressions, types, scans, pointer paths |
| [Command line](docs/wiki/en/cli.md) and [tool reference](docs/wiki/en/tools.md) | every command and tool |
| [Configuration](docs/wiki/en/configuration.md) | settings, files, environment |
| [Troubleshooting](docs/wiki/en/troubleshooting.md) | permissions, Wine/Proton, FAQ |
| [Development](docs/wiki/en/development.md) | architecture, adding tools, releases |

## How it works

```
 engram-gui ─┐
 engram CLI ─┼── Unix socket (0600, same uid) ──► engramd ──► /proc/<pid>/mem, ptrace
 engram mcp ─┘                                    one shared session
     ▲
     └── OpenCode / Claude Code / Codex / Gemini
```

`engramd` owns the session, and the window, the CLI and every agent are equal clients of it.
Tools that change the target are gated: an agent's request waits for your decision in the
window.

## Security

- Engram only attaches to processes of your own user. The socket is private (0700 directory,
  0600 socket) and the peer's uid is checked.
- Agents can't change memory, freeze values, pause the game, set watchpoints or start programs
  without your approval while the policy is *Ask me* (the default).
- `write` takes an `expected` value, so a stale address is rejected instead of corrupting memory.
- The only privileged piece is `CAP_SYS_PTRACE` on `engramd`, and only if you run `engram setup`.

Report vulnerabilities privately; see [SECURITY.md](SECURITY.md).

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) and the
[development guide](docs/wiki/en/development.md).

## License

[GPL-3.0-or-later](LICENSE)
