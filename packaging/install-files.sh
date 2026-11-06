#!/bin/sh
# Installs the program and its desktop files under a prefix: the binary,
# the menu entry, the skin MIME type, the AppStream metadata and the icons.
# The packages and install.sh share it, so each lists the same files.
# Usage: packaging/install-files.sh <binary> <prefix>   (e.g. "$pkg/usr")
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
bin=$1
prefix=$2

install -Dm755 "$bin" "$prefix/bin/popamp"
install -Dm644 "$here/popamp.desktop" "$prefix/share/applications/popamp.desktop"
install -Dm644 "$here/popamp.metainfo.xml" "$prefix/share/metainfo/popamp.metainfo.xml"
install -Dm644 "$here/popamp-skin.xml" "$prefix/share/mime/packages/popamp-skin.xml"
install -Dm644 "$root/assets/logo/popamp.svg" "$prefix/share/icons/hicolor/scalable/apps/popamp.svg"
for n in 16 24 32 48 64 128 256; do
    install -Dm644 "$root/assets/logo/png/popamp-$n.png" "$prefix/share/icons/hicolor/${n}x${n}/apps/popamp.png"
done
