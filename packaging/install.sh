#!/bin/sh
# Builds Popamp and installs it for the current user (~/.local).
set -eu
cd "$(dirname "$0")/.."
cargo build --release -p popamp
prefix="${PREFIX:-$HOME/.local}"
sh packaging/install-files.sh target/release/popamp "$prefix"
mkdir -p "$prefix/share/popamp/skins"
command -v update-mime-database >/dev/null && update-mime-database "$prefix/share/mime" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" || true
echo "Installed to $prefix/bin/popamp"
