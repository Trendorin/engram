# Installation

Engram runs on Linux x86-64. It has three programs, which are installed together:

| Program | Role |
|---|---|
| `engramd` | The engine. It holds the session and is the only part that reads other programs' memory |
| `engram` | Command line, MCP server for agents, `setup`, `doctor`, `config` and `uninstall` |
| `engram-gui` | The window (optional) |

## From a package

Every [release](https://github.com/Trendorin/engram/releases/latest) has packages for Arch Linux,
Debian 13+ and Fedora 44+, an AppImage for any distribution with glibc 2.35+, and `SHA256SUMS`.

```sh
sudo pacman -U ./engram-*-x86_64.pkg.tar.zst     # Arch, CachyOS, Manjaro
sudo apt install ./engram_*_amd64.deb            # Debian
sudo dnf install ./engram-*.x86_64.rpm           # Fedora
chmod +x Engram-*-x86_64.AppImage                # anything else
```

Packages put the programs into `/usr/bin` and remove cleanly with your package manager. The
AppImage opens the window; with arguments it is the `engram` command line
(`./Engram-*.AppImage doctor`, `./Engram-*.AppImage mcp` in an agent's MCP configuration).
Its files are read-only, so `engram setup` cannot grant memory access there: start games
through Engram instead.

## From source

You need Rust (stable, from [rustup.rs](https://rustup.rs)), a C++ compiler and, for the
native look, the Qt 6 development files:

| Distribution | Packages |
|---|---|
| Arch / CachyOS / Manjaro | `base-devel qt6-base` |
| Debian / Ubuntu | `build-essential qt6-base-dev qt6-base-dev-tools libfontconfig-dev libxkbcommon-dev` |
| Fedora | `gcc-c++ qt6-qtbase-devel fontconfig-devel libxkbcommon-devel` |

```sh
git clone https://github.com/Trendorin/engram
cd engram
./install.sh
```

The first build compiles the UI toolkit and takes several minutes. Later builds are fast.
If Qt 6 is not found, the window still builds, but it uses Slint's own style instead of your
system style.

## What the installer does

Run in a terminal, `./install.sh` is interactive:

1. checks your system (Rust, C++ compiler, Qt);
2. asks where to install (default `~/.local`), whether to install the window, and whether to
   grant memory access now;
3. builds (when run from source), showing progress;
4. copies the programs to `<prefix>/bin`, adds a menu entry and an icon, and writes an
   uninstall manifest `<prefix>/share/engram/installed-files`;
5. prepares the agent workspace (`~/.local/share/engram/workspace`);
6. optionally runs `engram setup`.

When Engram is already installed, it offers **Update**, **Uninstall** or **Quit**.

Options for scripts and CI:

| Option | Meaning |
|---|---|
| `--yes`, `-y` | No questions. Defaults plus the other flags |
| `--prefix DIR` | Install into `DIR/bin` and `DIR/share` |
| `--no-gui` | Engine, CLI and MCP server only |
| `--setup` / `--no-setup` | Grant memory access after installing, or don't ask |
| `--uninstall` | Remove Engram (same as `engram uninstall`) |
| `--purge` | With `--uninstall`, also delete settings, tables and logs |

The installer never runs `sudo` itself. Only `engram setup` asks for your password, through
pkexec (a desktop prompt) or sudo, for a single `setcap` on `engramd` (`cap_sys_ptrace=p`, the permitted form: the engine raises it itself only while attaching).

## Memory access: `engram setup`

Most distributions set Yama `ptrace_scope=1`: a program may read another program's memory only
if it started it. You have two options:

- **`engram setup`** (recommended). This grants `CAP_SYS_PTRACE` to `engramd` only. After that
  Engram can attach to any program of your user. The window and agents stay unprivileged.
- **Start games through Engram**: use the "Start a program" field in the window, or
  `engram launch -- <command>`. For Steam, set the launch option to `engram launch -- %command%`.

Replacing `engramd` (an update or reinstall) removes the capability, so run `engram setup` again.
`engram doctor` shows the current state.

## Updating

Run the new `install.sh` again and choose **Update**. Your settings and tables are kept.

## Uninstalling

```sh
engram uninstall           # asks first, keeps your data
engram uninstall --purge   # also deletes settings, saved tables, the agent workspace and logs
```

The uninstaller removes exactly the files listed in the manifest. It also stops this
installation's engine and window, but not other copies of Engram. If you registered Engram
with an agent globally (for example `claude mcp add engram`), remove that entry in the agent too.
