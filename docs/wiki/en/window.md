# The window

![Engram window](../../images/screenshot.png)

The window uses your Qt style and colour scheme, light or dark. Everything it shows is the
engine's shared session, so actions taken by an agent appear here live.

## Top bar

- **Select process…** opens the process list. Games come first, and Wine/Proton games appear
  under their Windows name with a `proton` or `wine` badge. Type to filter, then double-click
  a process or press Enter to attach. **Show runtime helpers** also lists `wineserver` and
  similar processes.
- **Or start a program** runs a command as a child of the engine. Engram can always read its
  own children, even without `engram setup`.
- **Pause / Resume** stops and continues the target (SIGSTOP/SIGCONT).
- **Detach** releases the target. Freezes stop.
- A yellow chip appears when Engram can only read programs it started. **Grant access…** runs
  `engram setup`.

## Approval bar

When an agent wants to write memory, freeze a value, pause the game, set a hardware
watchpoint or start a program, a yellow bar shows what it wants to do and why:

- **Allow** allows this one request.
- **Allow for session** lets this agent proceed without asking until the engine restarts or you
  change the policy.
- **Deny** refuses it. The agent is told not to retry.

Requests time out after 180 s; you can change this in Settings. Requests from the window
itself are never gated.

## Value scan

1. Type the value the game shows and press **First scan**, or just Enter. The type `auto`
   scans i32, i64, f32 and f64 at once and ranks the hits.
2. Change the value in the game, type the new value and press **Next scan**. Repeat until a
   few results remain. **Undo** goes back one step, and **New scan** starts over.
3. For values you can't see as a number (health bars), choose compare `unknown`, then use
   `decreased`, `increased` and `unchanged` as the value changes.

**Writable only** and **Aligned** apply to the first scan. Results update live. The **Where**
column shows `heap`, `stack` or `Module+offset`. For multi-type scans, a menu filters the
results by type. A long scan shows a progress bar with **Cancel**.

Double-click a result, or select it and press **Add to table**. Single-clicking a result puts
its address into the Inspect tab.

## Address table

The table is saved per game and reloaded when you attach to that game again.

- Tick **Freeze** to keep a value fixed. It is rewritten every 100 ms by default.
- Select a row to edit its description, address expression, type or value, then press
  **Apply** or Enter. With no row selected, the same fields add a new entry.
- Address expressions such as `Game.exe+1A2B30` or `[[Game.exe+10]+18]+40` keep working after
  the game restarts. Get them from **Pointer scan**.

Drag the line between the scan and the table, or between the left and right columns, to resize.

## Agent tab

- Pick an agent and press **Open terminal**. Engram prepares the workspace and opens your
  terminal there with the agent running. See [AI agents](agents.md).
- **Agent memory writes** sets the live policy: *Ask me*, *Allow all* or *Deny all*.
- **Activity** lists everything agents did, newest first. Failed calls are red, and messages
  from agents (`notify`) are bold.

## Inspect tab

Type an address or expression (or click a result), choose a type, then:

| Button | Shows |
|---|---|
| Analyze | region, module+offset, the bytes as every type, pointer target, text |
| Dissect | a guessed structure of 256 bytes from the address (start a little before your value) |
| Disassemble | 40 x86/x64 instructions |
| Hex | a 512-byte hex dump |
| Sample | the value 20 times over 3 s, with min, max and changes |
| Pointer scan | module-based paths to the address that survive restarts |
| Find writes / Find accesses | an 8 s hardware watchpoint. Trigger the change in the game while it runs |

**Add to table** saves the inspected address with the chosen type.

## Settings tab

Settings cover the agent, terminal and workspace folder, the policy at engine start, the
approval timeout, the number of results, and the refresh and freeze intervals. **Save**
applies them; **Restore defaults** fills in the defaults, which still need **Save**. The
*About* box shows the version, where settings, data and logs live (with **Open** buttons) and
the memory access mode. See [Configuration](configuration.md).
