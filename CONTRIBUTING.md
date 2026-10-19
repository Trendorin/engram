# Contributing to Engram

Thanks for helping. Bug reports, documentation fixes, translations and code are all welcome.

## Before you start

- For anything larger than a small fix, open an issue first so we can agree on the approach.
- Engram is for analysing your own programs and single-player games. Contributions that help
  bypass anti-cheat systems, hide Engram from other programs, or attach to other users'
  processes will not be accepted.

## Setup

You need Rust (stable), a C++ compiler and Qt 6 development files. See
[Installation → From source](docs/wiki/en/install.md#from-source).

```sh
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

The [development guide](docs/wiki/en/development.md) covers the architecture, how to add a
tool or a setting, and how to test by hand with the bundled fake game (`tools/testbed`)
without touching your real session.

## Guidelines

- Keep modules small, with a doc comment at the top of each file. Write `// SAFETY:` on
  every `unsafe` block.
- Don't add heavy dependencies (no async runtimes); the engine uses plain threads.
- Errors are `RpcError { code, message, hint }` with stable snake_case codes. The hint tells
  an agent (or a person) what to do next.
- Tool descriptions are written for language models: what it does, when to use it, what comes
  next.
- The window must never block on the engine. Calls go through the workers in `engine.rs`.
- User-visible text in the wiki exists in English and Russian; please update both, or say in
  the PR that a translation is missing.
- Note user-visible changes in `CHANGELOG.md` under *Unreleased*.

## Pull requests

Fill in the checklist in the PR template. CI runs formatting, clippy (warnings are errors),
the tests, and checks that `docs/wiki/en/tools.md` matches `engram tools --markdown`.

By contributing you agree that your work is licensed under GPL-3.0-or-later.
