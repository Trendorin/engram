# AI agents

Agents are full clients of the engine. They see the same session as the window, use the same
tools, and need your approval for anything that changes the target.

## The quick way

1. Open the window and attach to your game, or let the agent do it.
2. In the **Agent** tab, choose the agent and press **Open terminal**.
3. Ask for something, for example:

   > I have 37 arrows. Find them and double them.

The agent scans, asks you to change the value in the game ("shoot one arrow"), narrows the
results, saves the address to the table and asks to write 74. You click **Allow**.

**Open terminal** runs the agent in the workspace folder (default
`~/.local/share/engram/workspace`). It contains:

| File | For |
|---|---|
| `AGENTS.md` | The playbook agents read: rules, workflows, error codes |
| `CLAUDE.md` | Points Claude Code at `AGENTS.md` |
| `opencode.json` | OpenCode MCP configuration |
| `.mcp.json` | Claude Code (and other) MCP configuration |

Supported terminals: Konsole, GNOME Terminal, Console (kgx), Ptyxis, kitty, Alacritty,
Ghostty, WezTerm, foot, Xfce Terminal, Tilix and xterm, or any command you set in Settings.

## Another folder

```sh
engram agent-init ~/my-agent-folder
```

The command merges into existing files: other MCP servers and your own text in `AGENTS.md`
are kept.

## Manual MCP setup

The MCP server is `engram mcp` (stdio).

**OpenCode** (`opencode.json` in a project or `~/.config/opencode/opencode.json`):

```json
{ "mcp": { "engram": { "type": "local", "command": ["engram", "mcp"], "enabled": true } } }
```

OpenCode 2.x reads this format and converts it itself.

**Claude Code:**

```sh
claude mcp add engram -- engram mcp
```

**Codex:**

```sh
codex mcp add engram -- engram mcp
```

Agents without MCP can use the CLI. Every tool is `engram <tool> key=value` and prints compact
JSON. The `AGENTS.md` playbook explains both.

## Approvals and policy

| Tool | Needs approval when an agent calls it |
|---|---|
| `write` | always |
| `table_set` with `value` or `freeze=true` | always |
| `watch` | always (traces the game's threads briefly) |
| `process_control action=pause` | always |
| `launch` | always (not offered over MCP at all) |

The policy decides what happens:

- **ask** (default): the window shows the request. If no window is open, the request is
  refused with `approval_unavailable`.
- **allow**: agents may do it without asking. Use this only with agents you trust.
- **deny**: all such requests are refused.

Change it in the Agent tab (until the engine restarts), set the start-up value with
`engram config set policy ...`, or start the engine with `engramd --policy ...`.

## What agents see

The activity feed shows each call with its client name: `claude-code`, `opencode`,
`mcp:<client>` and so on. Agents report progress with `notify`. Their messages appear in bold
and in the status bar.

## Tips for good results

- Tell the agent which game and which value. Values only change when *you* play, so the
  agent will ask you to do things in the game.
- For durable cheats, ask for a pointer path ("make it survive a restart").
- With online games that have anti-cheat, the agent should refuse or warn. Engram is meant for
  single-player games and your own programs.
