# Command line

`engram` is the command line for people and agents. It starts the engine automatically.

## Commands

| Command | What it does |
|---|---|
| `engram <tool> key=value ...` | Call a tool. Prints JSON (pretty in a terminal) |
| `engram <tool> '<json>'` | Same, with JSON arguments |
| `engram tools` | List tools by group (`[approval]` = gated for agents) |
| `engram tools --markdown` | Print the [tool reference](tools.md) |
| `engram help <tool>` | Arguments, defaults and an example |
| `engram doctor` | Check binaries, memory access, the engine, agents and settings |
| `engram setup` | Grant memory access to the engine (asks for your password once) |
| `engram agent-init [dir]` | Write AGENTS.md and MCP configuration into a folder |
| `engram mcp` | Run the MCP server on stdio |
| `engram config ...` | Show and change [settings](configuration.md) |
| `engram launch -- <cmd> [args]` | Start a program as a child of the engine and wait for it |
| `engram events` | Stream live session events (JSON lines) |
| `engram daemon start\|stop\|status` | Manage the engine |
| `engram uninstall [--purge]` | Remove Engram ([details](install.md#uninstalling)) |
| `engram version` | Print the version |

`--pretty` / `--compact` force the JSON style. Exit code 1 means the tool returned an error,
which is printed as `{"error": {"code", "message", "hint"}}`.

## Arguments

`key=value` pairs are converted to JSON. Numbers and `true`/`false` become numbers and
booleans, and everything else stays a string. So `address=[[Game.exe+10]+18]` needs no
quoting beyond your shell's. For arrays and objects, pass one JSON argument.

```sh
engram processes query=minecraft
engram attach pid=12345
engram scan value=37
engram scan_next compare=decreased
engram scan_results limit=20
engram analyze address=0x55d0c3a17e50
engram table_add address=0x55d0c3a17e50 type=i32 description=Arrows
engram write '{"address":"0x55d0c3a17e50","value":"74","expected":"37","reason":"Double arrows"}'
engram pointer_scan address=0x55d0c3a17e50 max_depth=4
engram watch address=0x55d0c3a17e50 kind=write duration_ms=6000
```

Run from an agent's shell, the client name in the activity feed is detected (`claude-code`,
`opencode`, `codex`, `gemini`). Set `ENGRAM_CLIENT` to override it.

## Steam launch option

```
engram launch -- %command%
```

The game becomes a child of the engine, so Engram can read it without `engram setup`.
`engram launch` stays running as long as the game does, so Steam sees the real lifetime.
For Proton games, attach to the `Game.exe` process, not the launcher.

## Scripting

The CLI prints compact JSON when its output is not a terminal, so it combines well with `jq`:

```sh
engram scan_results limit=1000 | jq -r '.results[] | select(.where=="heap") | .address'
```
