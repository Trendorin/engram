#!/usr/bin/env bash
# Builds dist/Engram-<version>-x86_64.AppImage with Qt inside.
#
# Build on Ubuntu 22.04 (glibc 2.35): the AppImage then runs on that and anything
# newer. The release workflow uses GitHub's ubuntu-22.04 runner.
#
#   packaging/appimage/build.sh --deps   install build dependencies (apt, needs sudo), Rust and Qt
#   packaging/appimage/build.sh          build the AppImage
#
# Without arguments the AppImage opens the window; with arguments it is the `engram`
# command line (`Engram.AppImage mcp` for agents, `Engram.AppImage doctor`, ...).
set -euo pipefail

QT_VERSION=6.8.3
ROOT=$(cd "$(dirname "$(readlink -f "$0")")/../.." && pwd)
WORK=${ENGRAM_APPIMAGE_WORK:-$ROOT/build-appimage}
QT_DIR=$WORK/qt/$QT_VERSION/gcc_64
ID=io.github.trendorin.Engram

if [[ ${1:-} == --deps ]]; then
    sudo apt-get update -q
    sudo apt-get install -yq --no-install-recommends \
        build-essential gcc-12 g++-12 pkg-config patchelf file curl ca-certificates python3-pip \
        libfontconfig1-dev libfreetype6-dev libxkbcommon-dev libxkbcommon-x11-0 libgl-dev libegl-dev libwayland-dev \
        libdbus-1-3 libglib2.0-0 libx11-xcb1 libxcb-cursor0 libxcb-icccm4 libxcb-image0 libxcb-keysyms1 \
        libxcb-randr0 libxcb-render-util0 libxcb-shape0 libxcb-xinerama0 libxcb-xkb1 libxcb-xfixes0 \
        libxrender1 libsm6 libice6
    command -v cargo >/dev/null || curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal
    python3 -m pip install --user --quiet aqtinstall
    python3 -m aqt install-qt linux desktop "$QT_VERSION" linux_gcc_64 -O "$WORK/qt"
    exit 0
fi

[[ -x $QT_DIR/bin/qmake ]] || { echo "Qt $QT_VERSION not found in $QT_DIR: run $0 --deps first" >&2; exit 1; }
[[ -f $HOME/.cargo/env ]] && . "$HOME/.cargo/env"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
export CC=gcc-12 CXX=g++-12 QMAKE=$QT_DIR/bin/qmake CARGO_TARGET_DIR=$WORK/target

# Build tools are pinned to a release and checked against its SHA-256 before they run.
fetch_tool() { # URL SHA256 DIR
    local url=$1 sha=$2 file=$3/${1##*/}
    if [[ -x $file ]] && echo "$sha  $file" | sha256sum -c --status; then return; fi
    curl -fsSL --retry 3 -o "$file" "$url"
    echo "$sha  $file" | sha256sum -c --status || { echo "checksum mismatch for $url" >&2; rm -f "$file"; exit 1; }
    chmod +x "$file"
}

mkdir -p "$WORK/tools" "$ROOT/dist"
fetch_tool https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage \
    c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d "$WORK/tools"
fetch_tool https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/1-alpha-20250213-1/linuxdeploy-plugin-qt-x86_64.AppImage \
    15106be885c1c48a021198e7e1e9a48ce9d02a86dd0a1848f00bdbf3c1c92724 "$WORK/tools"

(cd "$ROOT" && cargo build --release --locked -p engramd -p engram -p engram-gui)

A=$WORK/AppDir
rm -rf "$A"
install -Dm755 -t "$A/usr/bin" "$WORK/target/release/engram-gui" "$WORK/target/release/engram" "$WORK/target/release/engramd"
install -Dm644 "$ROOT/packaging/$ID.desktop" -t "$A/usr/share/applications"
install -Dm644 "$ROOT/packaging/$ID.metainfo.xml" -t "$A/usr/share/metainfo"
install -Dm644 "$ROOT/packaging/$ID.svg" -t "$A/usr/share/icons/hicolor/scalable/apps"

# The engine outlives the window and the CLI, but the AppImage is unmounted when they
# exit. So it runs from a copy in the private runtime folder (only engramd's own code:
# it needs no Qt).
cat > "$WORK/AppRun" <<'APPRUN'
#!/bin/sh
HERE=$(dirname "$(readlink -f "$0")")
rt=${XDG_RUNTIME_DIR:-}
if [ -n "$rt" ] && [ -d "$rt" ] && [ -O "$rt" ]; then
    dir="$rt/engram-appimage"
    mkdir -p -m 700 "$dir" && [ -O "$dir" ] && {
        bin="$dir/engramd"
        cmp -s "$HERE/usr/bin/engramd" "$bin" || { cp "$HERE/usr/bin/engramd" "$bin.new" && chmod 755 "$bin.new" && mv -f "$bin.new" "$bin"; }
        export ENGRAMD_BIN="$bin"
    }
fi
if [ $# -gt 0 ]; then
    exec "$HERE/usr/bin/engram" "$@"
fi
exec "$HERE/usr/bin/engram-gui"
APPRUN
chmod +x "$WORK/AppRun"

export APPIMAGE_EXTRACT_AND_RUN=1 LD_LIBRARY_PATH=$QT_DIR/lib
export EXTRA_PLATFORM_PLUGINS="libqwayland-generic.so" LDAI_COMP=zstd
export LDAI_OUTPUT=$ROOT/dist/Engram-$VERSION-x86_64.AppImage
rm -f "$LDAI_OUTPUT"
cd "$WORK"
./tools/linuxdeploy-x86_64.AppImage --appdir "$A" \
    --executable "$A/usr/bin/engram-gui" \
    --executable "$A/usr/bin/engram" \
    --desktop-file "$A/usr/share/applications/$ID.desktop" \
    --icon-file "$A/usr/share/icons/hicolor/scalable/apps/$ID.svg" \
    --custom-apprun AppRun \
    --plugin qt --output appimage
echo "$LDAI_OUTPUT"
