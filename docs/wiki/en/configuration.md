# Configuration

Settings live in `~/.config/engram/config.json` (or `$XDG_CONFIG_HOME/engram`). Change them in
the window (**Settings** tab) or with `engram config`. Invalid or unknown values in the file
fall back to defaults, so a hand-edited file can't break Engram.

```sh
engram config                      # list all settings with descriptions
engram config get policy
engram config set refresh_ms 400
engram config reset freeze_interval_ms
engram config reset                # everything back to defaults
engram config path
```

| Key | Default | Accepts | Effect |
|---|---|---|---|
| `policy` | `ask` | `ask` `allow` `deny` | Agent writes when the engine starts. The window can change the live policy |
| `approval_timeout_s` | `180` | 10–3600 | How long an agent's request waits for your answer |
| `freeze_interval_ms` | `100` | 20–5000 | How often frozen values are rewritten |
| `agent` | (first found) | an agent name | Agent preselected for "Open terminal" (remembered when you use it) |
| `terminal` | (detect) | a command | Terminal emulator, optionally with its own arguments (`kitty --single-instance`) |
| `workspace` | `~/.local/share/engram/workspace` | a path | Folder agents are started in |
| `result_rows` | `1000` | 50–1000 | Scan results shown in the window |
| `refresh_ms` | `600` | 200–5000 | How often live values in the window refresh |

Saving in the window or with `engram config set` applies the timeout and the freeze interval
to a running engine immediately. `policy` applies at the next engine start.

## Files

| Path | Contents |
|---|---|
| `~/.config/engram/config.json` | Settings |
| `~/.local/share/engram/tables/<game>.json` | Saved address tables, one per game (Steam AppID included) |
| `~/.local/share/engram/workspace/` | Agent workspace |
| `~/.local/state/engram/engramd.log` | Engine log |
| `~/.local/state/engram/launch.log` | Output of programs started with `launch` |
| `$XDG_RUNTIME_DIR/engram/engramd.sock` | Engine socket (mode 0600, directory 0700) |
| `<prefix>/share/engram/installed-files` | Uninstall manifest |

## Environment variables

| Variable | Effect |
|---|---|
| `ENGRAM_POLICY` | Start-up policy for `engramd`. Overrides the setting; `--policy` overrides both |
| `ENGRAM_CLIENT` | Client name shown in the activity feed for CLI calls |
| `ENGRAM_SOCKET` | Use another socket path |
| `ENGRAMD_BIN`, `ENGRAM_BIN`, `ENGRAM_GUI_BIN` | Use a specific binary instead of the sibling / PATH lookup |
| `TERMINAL` | Preferred terminal when the `terminal` setting is empty |
| `NO_COLOR` | No colours in CLI output |
| `ENGRAM_GUI_SNAPSHOT` | Developer aid: the window saves images of itself to this folder |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR` | Standard locations |
