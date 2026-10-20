# Troubleshooting

Start with `engram doctor`. It checks the binaries, memory access, the engine, installed agents
and your settings.

## "permission_denied" when attaching

Your kernel restricts ptrace (Yama `ptrace_scope=1`). Either:

- run `engram setup` once (or **Grant access…** in the window), or
- start the game through Engram: the "Start a program" field, or `engram launch -- <command>`
  (Steam: `engram launch -- %command%`).

After updating or reinstalling, run `engram setup` again, because replacing `engramd` removes
its capability. With `ptrace_scope=3`, nothing can be traced until you reboot with a lower value.

## Agents' writes are refused

| Error | Meaning |
|---|---|
| `approval_unavailable` | The policy is *ask* and the window is not open. Open `engram-gui` |
| `approval_denied` | You pressed Deny, or the policy is *deny* |
| `approval_timeout` | Nobody answered in time (`approval_timeout_s`) |
| `stale_value` | Memory no longer holds `expected`. The agent should read again |

## Scans find nothing

- The type is wrong: try `auto`, or `f32`/`f64` for values with decimals.
- The number shown is scaled or rounded: scan `unknown`, then use `increased`/`decreased`.
- The value changed during the scan: scan again while the game is paused (**Pause**).
- The value is not in writable memory: untick **Writable only** (slower).

## Wine and Proton

- Attach to the `Game.exe` process (badge `proton`/`wine`), not to `wine64-preloader`,
  `steam.exe` or the Proton launcher. **Show runtime helpers** reveals the hidden ones.
- `.exe` and `.dll` images are modules, so paths look like `[[GameAssembly.dll+3A1F2C8]+B8]+40`.
- 32-bit games are detected automatically (4-byte pointers).
- Unity/.NET games often store text as UTF-16: use `string16`.

## The window

- **Plain-looking widgets instead of your theme**: the window was built without Qt 6.
  Install the Qt 6 development package and run `./install.sh` again.
- **"Engine offline"**: the window restarts the engine automatically. If it keeps failing, see
  `~/.local/state/engram/engramd.log`.
- **"Open terminal" does nothing**: set your terminal in Settings (`terminal`), for example
  `kitty` or `konsole`.
- The window doesn't start after a build from source: Qt 6 libraries are missing (`ldd
  $(which engram-gui) | grep "not found"`).

## FAQ

**Is this a cheat tool?** It is a memory scanner and debugger like Cheat Engine, PINCE or
GDB, meant for your own programs and single-player games. Using it in online games with
anti-cheat can get you banned. Don't.

**Can agents do anything without me?** They can read memory, scan and edit the table. Anything
that changes the game (writes, freezes, pausing, watchpoints, starting programs) waits for
your approval unless you set the policy to *allow*.

**Can it read other users' or root processes?** No. The engine only attaches to processes of
your own user, and only your user can talk to it.

**Does it work on Wayland?** Yes. The window uses Qt's Wayland support, and memory access
does not depend on the display server.
