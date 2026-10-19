# Concepts

## Address expressions

Wherever an address is expected (in the window, the CLI or a tool argument), you can write:

| Form | Example | Meaning |
|---|---|---|
| Number | `7f12a0`, `0x7f12a0` | Numbers are **hexadecimal** by default |
| Decimal | `#4096` | `#` marks a decimal number |
| Module + offset | `Game.exe+1A2B30`, `libgame.so+0x40` | Base address of a loaded module plus an offset |
| Quoted module | `"My Game.exe"+10` | Module names with spaces or dashes |
| Dereference | `[Game.exe+10]` | Read a pointer at that address |
| Pointer path | `[[Game.exe+1A2B30]+18]+40` | Follow pointers; survives restarts |
| Arithmetic | `0x7f12a0-0x40` | `+` and `-`, with parentheses |

Raw heap addresses change every time the game starts. Pointer paths from **Pointer scan**
don't, so store them in the table.

## Data types

| Type | Size | Notes |
|---|---|---|
| `i8` `i16` `i32` `i64` | 1–8 | Signed integers. `i32` is the most common for counters |
| `u8` `u16` `u32` `u64` | 1–8 | Unsigned integers |
| `f32` `f64` | 4 / 8 | Floats. Health, positions, timers |
| `ptr` | 4 or 8 | Pointer; 4 bytes for 32-bit games |
| `bytes:N` | N | Raw bytes, written as `90 90 90` |
| `str:N` | N | UTF-8 / ASCII text |
| `str16:N` | 2N | UTF-16 text (common in Unity/.NET and Windows games) |

Aliases are accepted too: `int`, `dword`, `float`, `double`, `byte` and so on.

## Scans

The first scan looks at all writable memory (or one module with `module=`). The next scan
filters the current candidates.

| Compare | First scan | Next scan | Needs value |
|---|---|---|---|
| `exact` | ✓ | ✓ | yes |
| `unknown` | ✓ | | no |
| `greater` / `less` | ✓ | ✓ | yes |
| `between` | ✓ | ✓ | two values |
| `not_equal` | ✓ | ✓ | yes |
| `changed` / `unchanged` | | ✓ | no |
| `increased` / `decreased` | | ✓ | no |
| `increased_by` / `decreased_by` | | ✓ | yes |

- The type **auto** scans i32, i64, f32 and f64 together. Candidates are ranked: typical
  counter types, the game's own memory and plausible float values come first.
- **Floats** match with rounding to the digits you typed. `100` matches 99.5…100.5, and
  `100.25` matches 100.245…100.255.
- **Byte patterns** accept `??` wildcards: `48 8B ?? 05`. **Text** searches use
  `string` or `string16`.
- If a value shown in the game is scaled (percent, ×10) or rounded, start with `unknown`
  and narrow down with `increased` / `decreased`.

## Pointer paths

A value usually lives in a heap object. Some static variable in the game's module points to
that object, possibly through a few more objects. **Pointer scan** searches backwards from
your address for chains up to depth 5 with offsets up to `max_offset`. It keeps only paths
that resolve to your address right now (✓ = verified).

To make an entry durable, copy a verified path into the entry's Address field, restart the
game, and check that it still shows the right value. Keep the one that survives.

## Hardware watchpoints

**Find writes / Find accesses** (`watch`) sets a CPU debug register on every thread of the
target for a few seconds. Each instruction that wrote or read the address is reported with a
hit count and the registers at the first hit. The memory operand (`[rcx+18]`) tells you which
register holds the object, and the instruction is where you would patch.

## Freezing

A frozen entry is rewritten periodically (`freeze_interval_ms`, default 100 ms). Freezes last
only for the session. They stop when you detach or the game exits, and they are not saved
with the table.
