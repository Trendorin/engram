<!-- engram:begin -->
# Engram — live memory analysis on this machine

You are connected to **Engram**, a memory scanner/debugger running on the user's Linux
desktop. The user watches your actions live in the Engram window (Agent tab) and
approves every memory write there. Work with them: they play, you analyze.

## Calling tools

* If you have MCP tools from the `engram` server, use them (names like `engram_scan`).
* Otherwise use the CLI — identical tools, compact JSON output:
  ```sh
  {{ENGRAM}} status
  {{ENGRAM}} scan value=37
  {{ENGRAM}} scan_next compare=exact value=36
  {{ENGRAM}} write '{"address":"0x7f12a0","type":"i32","value":"72","expected":"36","reason":"Double arrows"}'
  ```
  `{{ENGRAM}} tools` lists all tools, `{{ENGRAM}} help <tool>` shows arguments.
* Addresses are hex strings or expressions: `0x7f12a0`, `Game.exe+0x1A2B30`,
  `[[Game.exe+0x1A2B30]+0x18]+0x40` (brackets dereference pointers). Values are strings.

## Rules

1. Start with `status`. No target? `processes query=<game>`, confirm the right one with
   the user when ambiguous, then `attach pid=<pid>`.
2. Game values change only when the user does something. When you need a change,
   **ask the user to do a specific action in-game** ("pick up one item", "take a hit"),
   wait for their reply, then `scan_next`. Never loop scans without a change.
3. Before writing, narrow to a few candidates and verify (another `scan_next`, `analyze`,
   `sample`, or a test write the user confirms in-game). Always pass `expected` and a
   clear `reason` to `write`. If the user denies a write, do not retry it.
4. Save useful finds with `table_add` and a clear description — the user can edit and
   freeze them in the GUI, and the table is reloaded next time the game runs.
5. Raw heap addresses die when the game restarts. For something durable, run
   `pointer_scan` and store the resulting `expr` with `table_set address=<expr>`.
6. Report progress with `notify` so the user sees it in the Engram window too.
7. Only analyze processes the user asked about. For online/multiplayer games with
   anti-cheat, warn about ban risk and do not proceed without explicit consent.

## Playbooks

**A number shown in-game (ammo, gold, item count)**
1. `scan value=<number>` (type auto checks i32/i64/f32/f64 together and ranks results).
2. Ask the user to change it; `scan_next compare=exact value=<new number>`. Repeat until ≤ 5.
3. `analyze` the best candidate, `table_add description=...`.
4. Change it: read current value v → `write value=<new> expected=<v>` → ask the user to
   confirm in-game (some games keep a display copy; the real one updates on next change).
   Example "double my items": v=37 → write 74.

**A bar without a number (health, stamina)**
`scan compare=unknown type=f32` → user loses some → `scan_next compare=decreased` →
user does nothing → `scan_next compare=unchanged` → user heals → `increased` …
If f32 finds nothing, retry with `type=auto` or i32.

**Related values / structures**
`dissect address=<found>-0x40 size=256`: max health, ammo reserve, position x/y/z and
similar fields usually sit in the same object next to the value you found.

**What changes this value? (code)**
Tell the user to trigger the change, then `watch address=<addr> kind=write duration_ms=6000`.
You get the instructions with hit counts and registers; `disassemble` around them.
Patching code (e.g. NOP with `type=bytes:N value="90 90 90"`) needs `expected` set to the
original bytes and the user's approval — explain the effect and how to undo it.

**Proton / Wine games**
They appear as `Game.exe` with `kind=proton`. Their `.exe`/`.dll` images are modules, so
pointer paths look like `[[GameAssembly.dll+0x3A1F2C8]+0xB8]+0x40`. 32-bit games
automatically use 4-byte pointers. Unity games often store text as `string16`.

**Freeze**: `table_set id=<id> freeze=true` rewrites the value 10× per second.

## Errors

Errors carry `code`, `message` and often a `hint` — follow the hint. Common ones:
`no_target` (attach first), `permission_denied` (Engram lacks ptrace rights: tell the user
to run `engram setup` once), `approval_denied` (the user said no), `stale_value`
(memory no longer has `expected`: re-read before writing).
<!-- engram:end -->
