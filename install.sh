#!/usr/bin/env bash
# Engram installer.
#
#   ./install.sh                 interactive install or update (asks a few questions)
#   ./install.sh --yes           no questions: defaults plus the flags below
#   ./install.sh --prefix DIR    install into DIR/bin and DIR/share (default ~/.local)
#   ./install.sh --no-gui        engine, CLI and MCP server only
#   ./install.sh --setup         also grant memory access (runs `engram setup`, asks for your password)
#   ./install.sh --uninstall     remove Engram (add --purge to delete settings, tables and logs too)
#
# Builds from this source checkout with cargo. Prebuilt packages (Arch, Debian,
# Fedora, AppImage) are on the releases page. Never uses sudo itself: only
# `engram setup` asks for your password, for one `setcap` on the engine binary.

set -euo pipefail

# ---- options ------------------------------------------------------------------------

PREFIX="$HOME/.local"
GUI=1
SETUP=ask
ASSUME_YES=0
ACTION=install
PURGE=0

usage() { sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) PREFIX="${2:?--prefix needs a directory}"; shift 2 ;;
        --prefix=*) PREFIX="${1#--prefix=}"; shift ;;
        --no-gui) GUI=0; shift ;;
        --setup) SETUP=yes; shift ;;
        --no-setup) SETUP=no; shift ;;
        -y|--yes) ASSUME_YES=1; shift ;;
        --uninstall) ACTION=uninstall; shift ;;
        --purge) PURGE=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'unknown option: %s\n\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

INTERACTIVE=0
if [ "$ASSUME_YES" = 0 ] && [ -t 0 ] && [ -t 1 ]; then INTERACTIVE=1; fi

NAME=Engram SLUG=engram TAGLINE="memory scanner · AI agents · Linux"
ACCENT_RGB="110;86;240" ACCENT_256=99

# ---- look ---------------------------------------------------------------------------
# Shared by the installers of Engram, PkgScope and Sotto; only the name, tagline,
# accent colour and the build progress line differ.

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    R=$'\e[0m' B=$'\e[1m' D=$'\e[2m' RED=$'\e[31m' GRN=$'\e[32m' YEL=$'\e[33m' CLR=$'\e[K'
    case "${COLORTERM:-}" in
        truecolor | 24bit) ACC=$'\e[38;2;'"${ACCENT_RGB}m" ;;
        *) ACC=$'\e[38;5;'"${ACCENT_256}m" ;;
    esac
else
    R='' B='' D='' RED='' GRN='' YEL='' CLR='' ACC=''
fi
case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in *UTF-8* | *utf8*) UTF=1 ;; *) UTF=0 ;; esac

step() { printf '\n%s%s▸ %s%s\n' "$B" "$ACC" "$1" "$R"; }
ok() { printf '  %s✓%s %s\n' "$GRN" "$R" "$*"; }
warn() { printf '  %s!%s %s\n' "$YEL" "$R" "$*"; }
info() { printf '    %s%s%s\n' "$D" "$*" "$R"; }
die() { printf '\n  %s✗ %s%s\n\n' "$RED" "$*" "$R" >&2; exit 1; }
tilde() { case "$1" in "$HOME"/*) printf '~/%s' "${1#"$HOME"/}" ;; "$HOME") printf '~' ;; *) printf '%s' "$1" ;; esac; }

# desktop_entry SRC DEST BINARY — copies a .desktop file and points Exec and TryExec at BINARY,
# quoted the way the Desktop Entry spec wants, so paths with spaces or odd characters work and
# nothing in the path is ever interpreted as a program.
desktop_entry() {
    local src="$1" dst="$2" bin="$3" q line name="${3##*/}"
    q="${bin//\\/\\\\}"; q="${q//\"/\\\"}"; q="${q//\$/\\\$}"; q="${q//\`/\\\`}"; q="${q//%/%%}"
    while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
            "Exec=$name" | "Exec=$name "*) printf 'Exec="%s"%s\n' "$q" "${line#"Exec=$name"}" ;;
            "TryExec=$name") printf 'TryExec=%s\n' "$bin" ;;
            *) printf '%s\n' "$line" ;;
        esac
    done <"$src" >"$dst"
}

# inside_prefix PATH — true if PATH (a line of the uninstall manifest) really is below $PREFIX:
# no "..", and its directory resolves, symlinks included, to somewhere inside the prefix.
inside_prefix() {
    local f="$1" d root
    case "$f" in */../* | */..) return 1 ;; esac
    d="$(cd "$(dirname "$f")" 2>/dev/null && pwd -P)" || return 1
    root="$(cd "$PREFIX" 2>/dev/null && pwd -P)" || return 1
    case "$d/" in "$root"/*) return 0 ;; *) return 1 ;; esac
}

# Closed input is never taken as an answer: stop before doing anything else.
read_answer() {
    if ! read -r REPLY; then
        printf '\n\n  %sNo answer (input closed) — stopping.%s\n\n' "$YEL" "$R" >&2
        exit 1
    fi
}

# ask_yn "question" y|n → exit status 0 for yes
ask_yn() {
    local q="$1" def="$2" hint ans
    if [ "$INTERACTIVE" = 0 ]; then [ "$def" = y ]; return; fi
    if [ "$def" = y ]; then hint="Y/n"; else hint="y/N"; fi
    printf '  %s?%s %s %s[%s]%s ' "$ACC" "$R" "$q" "$D" "$hint" "$R"
    read_answer
    ans="$(printf '%s' "$REPLY" | tr '[:upper:]' '[:lower:]')"
    case "$ans" in
        "") [ "$def" = y ] ;;
        y* | д*) return 0 ;;
        *) return 1 ;;
    esac
}

# ask_text "question" default → prints the answer
ask_text() {
    local q="$1" def="$2" ans
    if [ "$INTERACTIVE" = 0 ]; then printf '%s' "$def"; return; fi
    printf '  %s?%s %s %s[%s]%s ' "$ACC" "$R" "$q" "$D" "$(tilde "$def")" "$R" >&2
    read_answer
    ans="$REPLY"
    case "$ans" in "~"/*) ans="$HOME/${ans#"~/"}" ;; esac
    printf '%s' "${ans:-$def}"
}

# choose "question" option... → prints the chosen number (1-based)
choose() {
    local q="$1" i=1 ans
    shift
    printf '  %s?%s %s\n' "$ACC" "$R" "$q" >&2
    for opt in "$@"; do printf '    %s%d)%s %s\n' "$B" "$i" "$R" "$opt" >&2; i=$((i + 1)); done
    while true; do
        printf '    %s>%s ' "$ACC" "$R" >&2
        read_answer
        ans="${REPLY:-1}"
        if [ "$ans" -ge 1 ] 2>/dev/null && [ "$ans" -le $# ]; then printf '%s' "$ans"; return; fi
    done
}

banner() {
    local bar='▍' w=48 title="$NAME $1"
    [ "$UTF" = 1 ] || bar='|'
    printf '\n  %s╭%s╮%s\n' "$D" "$(printf '─%.0s' $(seq $w))" "$R"
    printf '  %s│%s  %s%s%s %s%s%s %s%s%s%*s%s│%s\n' "$D" "$R" "$ACC" "$bar" "$R" "$B" "$NAME" "$R" "$D" "$1" "$R" \
        $((w - 4 - ${#title})) "" "$D" "$R"
    printf '  %s│%s  %s%s%s %s%*s%s│%s\n' "$D" "$R" "$ACC" "$bar" "$R" "$TAGLINE" $((w - 4 - ${#TAGLINE})) "" "$D" "$R"
    printf '  %s╰%s╯%s\n' "$D" "$(printf '─%.0s' $(seq $w))" "$R"
}

# spin "label" command... — runs the command with a progress line; output goes to a log.
# progress LOG (defined per project) prints what the build is doing right now.
spin() {
    local label="$1" log status frames='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏' i=0 start=$SECONDS now
    shift
    [ "$UTF" = 1 ] || frames='|/-\|/-\||'
    log="$(mktemp "${TMPDIR:-/tmp}/$SLUG-install.XXXXXX")"
    "$@" >"$log" 2>&1 &
    local pid=$!
    if [ -t 1 ]; then
        while kill -0 "$pid" 2>/dev/null; do
            now="$(progress "$log" 2>/dev/null || true)"
            printf '\r  %s%s%s %s %s%s%ss%s%s' "$ACC" "${frames:i++%10:1}" "$R" "$label" "$D" "${now:+$now · }" "$((SECONDS - start))" "$R" "$CLR"
            sleep 0.12
        done
        printf '\r%s' "$CLR"
    fi
    status=0
    wait "$pid" || status=$?
    if [ "$status" -ne 0 ]; then
        printf '  %s✗%s %s failed. Last lines of the log:\n\n' "$RED" "$R" "$label"
        tail -n 25 "$log" | sed 's/^/    /'
        printf '\n    Full log: %s\n\n' "$log"
        exit 1
    fi
    rm -f "$log"
    local t=$((SECONDS - start))
    ok "$label done in $((t / 60))m $((t % 60))s"
}

# progress for CMake + Ninja/Make builds: "[12/40]" or "[ 30%]"
cmake_progress() {
    grep -oE '^\[ *[0-9]+(/[0-9]+|%)\]' "$1" | tail -1 | tr -d '[] '
}

# Distribution family for package hints: arch, debian, fedora or other.
distro() {
    local id like
    id="$(. /etc/os-release 2>/dev/null && printf '%s' "${ID:-}")"
    like="$(. /etc/os-release 2>/dev/null && printf '%s' "${ID_LIKE:-}")"
    case " $id $like " in
        *" arch "*) printf arch ;;
        *" debian "* | *" ubuntu "*) printf debian ;;
        *" fedora "* | *" rhel "*) printf fedora ;;
        *) printf other ;;
    esac
}

# need_packages "what" arch-pkgs debian-pkgs fedora-pkgs → stops with the right install command
need_packages() {
    local what="$1" cmd
    case "$(distro)" in
        arch) cmd="sudo pacman -S --needed $2" ;;
        debian) cmd="sudo apt install $3" ;;
        fedora) cmd="sudo dnf install $4" ;;
        *) cmd="" ;;
    esac
    printf '\n  %s✗ %s%s\n' "$RED" "$what" "$R" >&2
    [ -n "$cmd" ] && printf '\n    Install them with:\n      %s%s%s\n' "$B" "$cmd" "$R" >&2
    printf '\n    Then run ./install.sh again.\n\n' >&2
    exit 1
}

# progress for cargo: crates compiled so far and the current one
progress() {
    local n cur
    n="$(grep -c '^ *Compiling ' "$1" || true)"
    cur="$(grep -o 'Compiling [^ ]*' "$1" | tail -1 | cut -d' ' -f2 || true)"
    printf '%s crates%s' "$n" "${cur:+ · $cur}"
}

# ---- where things are -----------------------------------------------------------

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[ -f "$SRC_DIR/Cargo.toml" ] && [ -d "$SRC_DIR/crates" ] || die "run install.sh from the Engram source folder"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$SRC_DIR/Cargo.toml" | head -1)"
APP_ID=io.github.trendorin.Engram
ICON_SRC="$SRC_DIR/packaging/$APP_ID.svg"
DESKTOP_SRC="$SRC_DIR/packaging/$APP_ID.desktop"

set_paths() {
    case "$PREFIX" in /*) ;; *) PREFIX="$PWD/$PREFIX" ;; esac
    PREFIX="${PREFIX%/}"
    BIN_DIR="$PREFIX/bin"
    APP_DIR="$PREFIX/share/applications"
    ICON_DIR="$PREFIX/share/icons/hicolor/scalable/apps"
    MANIFEST="$PREFIX/share/engram/installed-files"
}
set_paths

installed_version() {
    if [ -x "$BIN_DIR/engram" ]; then "$BIN_DIR/engram" --version 2>/dev/null | awk '{print $2}'; fi
}

# ---- uninstall --------------------------------------------------------------------------

uninstall() {
    step "Uninstall"
    local args=()
    [ "$ASSUME_YES" = 1 ] && args+=(--yes)
    [ "$PURGE" = 1 ] && args+=(--purge)
    if [ -x "$BIN_DIR/engram" ] && [ -f "$MANIFEST" ]; then
        exec "$BIN_DIR/engram" uninstall "${args[@]}"
    fi
    # Installs from before the manifest existed: remove the known files.
    local files=("$BIN_DIR/engram" "$BIN_DIR/engramd" "$BIN_DIR/engram-gui" "$APP_DIR/engram.desktop" "$ICON_DIR/engram.svg"
                 "$APP_DIR/$APP_ID.desktop" "$ICON_DIR/$APP_ID.svg")
    local found=0 f
    for f in "${files[@]}"; do if [ -e "$f" ]; then found=1; fi; done
    [ "$found" = 1 ] || { ok "Engram is not installed in $(tilde "$PREFIX")"; exit 0; }
    ask_yn "Remove Engram from $(tilde "$PREFIX")?" y || { info "Nothing changed."; exit 0; }
    "$BIN_DIR/engram" daemon stop >/dev/null 2>&1 || true
    for f in "${files[@]}"; do
        if [ -e "$f" ]; then rm -f "$f"; ok "removed $(tilde "$f")"; fi
    done
    if [ "$PURGE" = 1 ] || { [ "$ASSUME_YES" = 0 ] && ask_yn "Also delete your settings, saved tables and logs?" n; }; then
        local d
        for d in "${XDG_CONFIG_HOME:-$HOME/.config}/engram" "${XDG_DATA_HOME:-$HOME/.local/share}/engram" "${XDG_STATE_HOME:-$HOME/.local/state}/engram"; do
            if [ -d "$d" ]; then rm -rf "$d"; ok "removed $(tilde "$d")"; fi
        done
    fi
    ok "Engram is uninstalled."
}

# ---- checks --------------------------------------------------------------------------------

QT_NATIVE=0
check_system() {
    step "Checking your system"
    [ "$(uname -s)" = Linux ] || die "Engram runs on Linux only"
    ok "Linux $(uname -m)"
    command -v cargo >/dev/null 2>&1 || die "Rust is not installed. Get it from https://rustup.rs and run this again."
    ok "Rust $(cargo --version | awk '{print $2}')"
    if [ "$GUI" = 1 ]; then
        command -v c++ >/dev/null 2>&1 || command -v g++ >/dev/null 2>&1 || command -v clang++ >/dev/null 2>&1 \
            || die "a C++ compiler is needed for the window (install gcc or clang), or use --no-gui"
        local q="${QMAKE:-}" cand
        if [ -z "$q" ]; then
            for cand in qmake6 /usr/lib/qt6/bin/qmake /usr/lib64/qt6/bin/qmake qmake-qt6 qmake; do
                if command -v "$cand" >/dev/null 2>&1 && "$cand" -query QT_VERSION 2>/dev/null | grep -q '^6\.'; then
                    q="$(command -v "$cand")"
                    break
                fi
            done
        fi
        if [ -n "$q" ]; then
            export QMAKE="$q"
            QT_NATIVE=1
            ok "Qt $("$q" -query QT_VERSION) — the window uses your system style"
        else
            warn "Qt 6 not found — the window will use Slint's own style"
            info "For the native look install qt6-base (Arch), qt6-base-dev (Debian/Ubuntu) or qt6-qtbase-devel (Fedora)."
        fi
    fi
}

NEEDS_ACCESS=0
check_access() {
    local scope cap=0
    scope="$(cat /proc/sys/kernel/yama/ptrace_scope 2>/dev/null || echo 0)"
    if command -v getcap >/dev/null 2>&1 && [ -e "$BIN_DIR/engramd" ] && getcap "$BIN_DIR/engramd" 2>/dev/null | grep -q cap_sys_ptrace; then
        cap=1
    fi
    # Replacing engramd drops its capability, so an update needs setup again too.
    if [ "$scope" = 1 ] || [ "$scope" = 2 ] || [ "$cap" = 1 ]; then NEEDS_ACCESS=1; fi
    if [ "$scope" = 3 ]; then warn "the kernel forbids ptrace entirely (ptrace_scope=3); Engram cannot read other programs"; fi
}

# ---- install --------------------------------------------------------------------------------

build() {
    step "Building"
    local pkgs=(-p engramd -p engram)
    [ "$GUI" = 1 ] && pkgs+=(-p engram-gui)
    info "the first build compiles the UI toolkit and takes a few minutes"
    spin "Release build" bash -c 'cd "$1" && shift && exec cargo build --release --locked "$@"' _ "$SRC_DIR" "${pkgs[@]}"
    local target
    target="$(cd "$SRC_DIR" && cargo metadata --format-version 1 --no-deps 2>/dev/null | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
    OUT="${target:-$SRC_DIR/target}/release"
}

install_files() {
    step "Installing into $(tilde "$PREFIX")"
    local src_bin="$OUT"
    mkdir -p "$BIN_DIR" "$PREFIX/share/engram" || die "cannot create $BIN_DIR"
    [ -w "$BIN_DIR" ] || die "$(tilde "$BIN_DIR") is not writable — choose another location"

    # The engine keeps running the old binary until it restarts.
    if [ -x "$BIN_DIR/engram" ]; then timeout 5 "$BIN_DIR/engram" daemon stop >/dev/null 2>&1 || true; fi

    local files=() b
    for b in engramd engram; do
        install -m755 "$src_bin/$b" "$BIN_DIR/$b"
        files+=("$BIN_DIR/$b")
    done
    if [ "$GUI" = 1 ]; then
        install -m755 "$src_bin/engram-gui" "$BIN_DIR/engram-gui"
        install -Dm644 "$ICON_SRC" "$ICON_DIR/$APP_ID.svg"
        mkdir -p "$APP_DIR"
        desktop_entry "$DESKTOP_SRC" "$APP_DIR/$APP_ID.desktop" "$BIN_DIR/engram-gui"
        files+=("$BIN_DIR/engram-gui" "$ICON_DIR/$APP_ID.svg" "$APP_DIR/$APP_ID.desktop")
    else
        rm -f "$BIN_DIR/engram-gui" "$ICON_DIR/$APP_ID.svg" "$APP_DIR/$APP_ID.desktop"
    fi
    # Names used by Engram 0.1 before the app id.
    rm -f "$ICON_DIR/engram.svg" "$APP_DIR/engram.desktop"
    files+=("$MANIFEST")
    printf '%s\n' "${files[@]}" > "$MANIFEST"

    ok "engramd, engram$([ "$GUI" = 1 ] && printf ', engram-gui') → $(tilde "$BIN_DIR")"
    [ "$GUI" = 1 ] && ok "menu entry and icon"
    ok "uninstall manifest"
    if pgrep -x engram-gui >/dev/null 2>&1; then warn "an Engram window is open — restart it to use the new version"; fi
}

prepare_workspace() {
    local ws
    ws="$("$BIN_DIR/engram" config get workspace 2>/dev/null || true)"
    case "$ws" in
        "") ws="${XDG_DATA_HOME:-$HOME/.local/share}/engram/workspace" ;;
        "~"/*) ws="$HOME/${ws#"~/"}" ;;
    esac
    if "$BIN_DIR/engram" agent-init "$ws" >/dev/null 2>&1; then
        ok "agent workspace $(tilde "$ws")"
    else
        warn "could not prepare the agent workspace (later: engram agent-init $(tilde "$ws"))"
    fi
}

grant_access() {
    step "Memory access"
    if "$BIN_DIR/engram" setup; then :; else warn "not granted — run 'engram setup' later"; fi
}

summary() {
    local on_path=1
    case ":$PATH:" in *":$BIN_DIR:"*) ;; *) on_path=0 ;; esac
    local e="engram" g="engram-gui"
    if [ "$on_path" = 0 ]; then e="$(tilde "$BIN_DIR")/engram"; g="$(tilde "$BIN_DIR")/engram-gui"; fi
    printf '\n  %s%s✓ Engram %s is installed%s\n\n' "$B" "$GRN" "$VERSION" "$R"
    [ "$GUI" = 1 ] && printf '    %-14s %s  %s(or "Engram" in your app menu)%s\n' "Open" "$g" "$D" "$R"
    printf '    %-14s %s doctor\n' "Check" "$e"
    printf '    %-14s %s config\n' "Settings" "$e"
    printf '    %-14s %s uninstall\n' "Uninstall" "$e"
    if [ "$SETUP" != yes ] && [ "$NEEDS_ACCESS" = 1 ]; then
        printf '\n    %sEngram can only read programs it starts until you run:%s %s setup\n' "$YEL" "$R" "$e"
    fi
    if [ "$on_path" = 0 ]; then
        printf '\n    %s%s is not on your PATH.%s Add it in your shell profile, e.g.\n' "$YEL" "$(tilde "$BIN_DIR")" "$R"
        printf '      export PATH="%s:$PATH"\n' "$BIN_DIR"
    fi
    printf '\n'
}

# ---- main --------------------------------------------------------------------------------------

banner "$VERSION"

if [ "$ACTION" = uninstall ]; then
    uninstall
    exit 0
fi

current="$(installed_version || true)"
if [ -n "$current" ] && [ "$INTERACTIVE" = 1 ]; then
    printf '\n  Engram %s is installed in %s.\n' "$current" "$(tilde "$PREFIX")"
    pick="$(choose "What do you want to do?" "Update to $VERSION" "Uninstall" "Quit")"
    case "$pick" in
        2) uninstall; exit 0 ;;
        3) exit 0 ;;
    esac
fi

check_system
check_access

if [ "$INTERACTIVE" = 1 ]; then
    step "A few questions"
    if [ -z "$current" ]; then
        PREFIX="$(ask_text "Install location" "$PREFIX")"
        set_paths
    fi
    if [ "$GUI" = 1 ] && ! ask_yn "Install the desktop window (engram-gui)?" y; then GUI=0; fi
    if [ "$SETUP" = ask ] && [ "$NEEDS_ACCESS" = 1 ]; then
        info "Linux only lets a program read memory of programs it started itself."
        info "Engram can get permission once for its engine (asks for your password)."
        if ask_yn "Grant memory access after installing?" y; then SETUP=yes; else SETUP=no; fi
    fi
fi
[ "$SETUP" = ask ] && SETUP=no

build
install_files
prepare_workspace
if [ "$SETUP" = yes ] && [ "$NEEDS_ACCESS" = 1 ]; then
    grant_access
elif [ "$SETUP" = yes ]; then
    ok "memory access: not needed on this system"
fi
summary
