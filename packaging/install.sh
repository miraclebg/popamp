#!/bin/sh
# Builds Popamp and installs it for the current user (~/.local).
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p popamp
prefix="${PREFIX:-$HOME/.local}"
install -Dm755 target/release/popamp "$prefix/bin/popamp"
install -Dm644 packaging/popamp.desktop "$prefix/share/applications/popamp.desktop"
install -Dm644 packaging/popamp-skin.xml "$prefix/share/mime/packages/popamp-skin.xml"
install -Dm644 packaging/popamp.metainfo.xml "$prefix/share/metainfo/popamp.metainfo.xml"
install -Dm644 assets/logo/popamp.svg "$prefix/share/icons/hicolor/scalable/apps/popamp.svg"
for n in 16 24 32 48 64 128 256; do
    install -Dm644 "assets/logo/png/popamp-$n.png" "$prefix/share/icons/hicolor/${n}x${n}/apps/popamp.png"
done
mkdir -p "$prefix/share/popamp/skins"
command -v update-mime-database >/dev/null && update-mime-database "$prefix/share/mime" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" || true
echo "Installed to $prefix/bin/popamp"
