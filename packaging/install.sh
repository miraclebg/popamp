#!/bin/sh
# Builds Linamp and installs it for the current user (~/.local).
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p linamp
prefix="${PREFIX:-$HOME/.local}"
install -Dm755 target/release/linamp "$prefix/bin/linamp"
install -Dm644 packaging/linamp.desktop "$prefix/share/applications/linamp.desktop"
install -Dm644 packaging/linamp-skin.xml "$prefix/share/mime/packages/linamp-skin.xml"
install -Dm644 packaging/linamp.metainfo.xml "$prefix/share/metainfo/linamp.metainfo.xml"
install -Dm644 assets/logo/linamp.svg "$prefix/share/icons/hicolor/scalable/apps/linamp.svg"
for n in 16 24 32 48 64 128 256; do
    install -Dm644 "assets/logo/png/linamp-$n.png" "$prefix/share/icons/hicolor/${n}x${n}/apps/linamp.png"
done
mkdir -p "$prefix/share/linamp/skins"
command -v update-mime-database >/dev/null && update-mime-database "$prefix/share/mime" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" || true
echo "Installed to $prefix/bin/linamp"
