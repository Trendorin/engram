#!/usr/bin/env bash
# Builds dist/engram_<version>-1_<arch>.deb from this source tree on Debian 13 or newer.
#
#   sudo apt-get install build-essential dpkg-dev qt6-base-dev libfontconfig-dev libxkbcommon-dev
#   (plus Rust 1.88 or newer from rustup.rs)
#   packaging/deb/build.sh
#   sudo apt install ./dist/engram_*.deb
set -euo pipefail

ROOT=$(cd "$(dirname "$(readlink -f "$0")")/../.." && pwd)
cd "$ROOT"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH=$(dpkg --print-architecture)
ID=io.github.trendorin.Engram
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
PKG=$STAGE/engram
export CARGO_TARGET_DIR=$ROOT/target
# The window's native style: Slint finds Qt through qmake.
if [[ -z ${QMAKE:-} ]]; then
    for q in qmake6 /usr/lib/qt6/bin/qmake; do
        if command -v "$q" >/dev/null; then export QMAKE=$(command -v "$q"); break; fi
    done
fi

cargo build --release --locked -p engramd -p engram -p engram-gui
cargo test --release --locked -p engram-core -p engram-proto -p engramd -p engram

install -Dm755 -t "$PKG/usr/bin" target/release/engramd target/release/engram target/release/engram-gui
install -Dm644 "packaging/$ID.desktop" -t "$PKG/usr/share/applications"
install -Dm644 "packaging/$ID.metainfo.xml" -t "$PKG/usr/share/metainfo"
install -Dm644 "packaging/$ID.svg" -t "$PKG/usr/share/icons/hicolor/scalable/apps"
install -Dm644 LICENSE "$PKG/usr/share/doc/engram/copyright"

mkdir -p "$STAGE/debian"
printf 'Source: engram\n\nPackage: engram\nArchitecture: any\n' > "$STAGE/debian/control"
SHLIBS=$(cd "$STAGE" && dpkg-shlibdeps -O -e engram/usr/bin/engramd -e engram/usr/bin/engram -e engram/usr/bin/engram-gui \
    | sed -n 's/^shlibs:Depends=//p')

install -d "$PKG/DEBIAN"
cat > "$PKG/DEBIAN/control" <<CONTROL
Package: engram
Version: $VERSION-1
Architecture: $ARCH
Maintainer: Trendorin <193045225+Trendorin@users.noreply.github.com>
Installed-Size: $(du -sk "$PKG" | cut -f1)
Depends: $SHLIBS
Recommends: libcap2-bin, pkexec
Section: devel
Priority: optional
Homepage: https://github.com/Trendorin/engram
Description: memory scanner and debugger that AI agents can drive
 Engram finds values such as health, gold or ammo in a running program,
 changes or freezes them, finds the code that writes them and saves pointer
 paths that survive restarts. It works with native Linux programs and Windows
 games under Wine and Proton. AI coding agents are full clients through an MCP
 server and a CLI, and every memory write waits for the user's approval.
 Engram only attaches to ordinary processes of the user's own account.
CONTROL
cat > "$PKG/DEBIAN/postinst" <<'SCRIPT'
#!/bin/sh
set -e
if [ "$1" = configure ]; then
    echo "Engram: run 'engram setup' once to let it read your own games (grants CAP_SYS_PTRACE"
    echo "to /usr/bin/engramd, asks for your password). Without it, start games through Engram."
fi
SCRIPT
chmod 0755 "$PKG/DEBIAN/postinst"

mkdir -p dist
OUT=dist/engram_${VERSION}-1_${ARCH}.deb
dpkg-deb --root-owner-group -Zxz --build "$PKG" "$OUT"
echo "$OUT"
