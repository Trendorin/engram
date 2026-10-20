# Rust binaries are built stripped (profile.release); there is no debug info to split.
%global debug_package %{nil}
# Slint compiles C++ (its Qt backend) that rustc links: no distribution LTO flags.
%global _lto_cflags %{nil}

Name:           engram
Version:        1.1.0
Release:        1%{?dist}
Summary:        Memory scanner and debugger for Linux that AI agents can drive
License:        GPL-3.0-or-later
URL:            https://github.com/Trendorin/engram
Source0:        %{url}/releases/download/v%{version}/%{name}-%{version}.tar.gz

BuildRequires:  cargo >= 1.88
BuildRequires:  rust >= 1.88
BuildRequires:  gcc-c++
BuildRequires:  qt6-qtbase-devel
BuildRequires:  fontconfig-devel
BuildRequires:  libxkbcommon-devel
Requires:       hicolor-icon-theme
Recommends:     libcap

%description
Engram finds values such as health, gold or ammo in a running program, changes
or freezes them, finds the code that writes them and saves pointer paths that
survive restarts. It works with native Linux programs and Windows games under
Wine and Proton. AI coding agents are full clients through an MCP server and a
CLI, and every memory write waits for the user's approval. Engram only attaches
to ordinary processes of the user's own account.

%prep
%autosetup

%build
export QMAKE=$(command -v qmake6 || command -v qmake-qt6)
cargo build --release --locked -p engramd -p engram -p engram-gui

%check
cargo test --release --locked -p engram-core -p engram-proto -p engramd -p engram

%install
install -Dm755 -t %{buildroot}%{_bindir} target/release/engramd target/release/engram target/release/engram-gui
install -Dm644 packaging/io.github.trendorin.Engram.desktop -t %{buildroot}%{_datadir}/applications
install -Dm644 packaging/io.github.trendorin.Engram.metainfo.xml -t %{buildroot}%{_datadir}/metainfo
install -Dm644 packaging/io.github.trendorin.Engram.svg -t %{buildroot}%{_datadir}/icons/hicolor/scalable/apps

%post
echo "Engram: run 'engram setup' once to let it read your own games (grants CAP_SYS_PTRACE"
echo "to %{_bindir}/engramd, asks for your password). Without it, start games through Engram."

%files
%license LICENSE
%doc README.md CHANGELOG.md
%{_bindir}/engramd
%{_bindir}/engram
%{_bindir}/engram-gui
%{_datadir}/applications/io.github.trendorin.Engram.desktop
%{_datadir}/metainfo/io.github.trendorin.Engram.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.trendorin.Engram.svg

%changelog
* Tue Oct 20 2026 Trendorin <193045225+Trendorin@users.noreply.github.com> - 1.1.0-1
- Language setting (English, Russian), Russian window

* Tue Oct 20 2026 Trendorin <193045225+Trendorin@users.noreply.github.com> - 1.0.1-1
- Security and correctness fixes, see CHANGELOG.md

* Tue Oct 20 2026 Trendorin <193045225+Trendorin@users.noreply.github.com> - 1.0.0-1
- First release
