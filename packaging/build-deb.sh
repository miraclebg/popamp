#!/bin/sh
# Builds a .deb package from an already-compiled release binary.
# Usage: packaging/build-deb.sh [target/release/popamp] [output-dir]
# Needs: dpkg-deb, objdump and strip (binutils). Version is read from Cargo.toml.
# Adapted from Ferramp's build-deb.sh (see THIRD_PARTY.md).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
bin=${1:-"$root/target/release/popamp"}
outdir=${2:-"$root/dist"}

if [ ! -x "$bin" ]; then
    echo "error: binary not found or not executable: $bin" >&2
    echo "hint: run 'cargo build --release -p popamp' first" >&2
    exit 1
fi

version=$(grep -m1 '^version' "$root/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
case "$(uname -m)" in
    x86_64) debarch=amd64 ;;
    aarch64|arm64) debarch=arm64 ;;
    *) echo "error: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac
# The newest glibc symbol the binary uses: depends on the build machine.
glibc=$(objdump -T "$bin" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)

stagedir=$(mktemp -d)
trap 'rm -rf "$stagedir"' EXIT INT TERM
pkg="$stagedir/popamp"
doc="$pkg/usr/share/doc/popamp"

install -Dm755 -s "$bin" "$pkg/usr/bin/popamp"
install -Dm644 "$here/popamp.desktop" "$pkg/usr/share/applications/popamp.desktop"
install -Dm644 "$here/popamp.metainfo.xml" "$pkg/usr/share/metainfo/popamp.metainfo.xml"
install -Dm644 "$here/popamp-skin.xml" "$pkg/usr/share/mime/packages/popamp-skin.xml"
install -Dm644 "$root/assets/logo/popamp.svg" "$pkg/usr/share/icons/hicolor/scalable/apps/popamp.svg"
for n in 16 24 32 48 64 128 256; do
    install -Dm644 "$root/assets/logo/png/popamp-$n.png" "$pkg/usr/share/icons/hicolor/${n}x${n}/apps/popamp.png"
done
for f in README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md; do
    install -Dm644 "$root/$f" "$doc/$f"
done
cat > "$doc/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Source: https://github.com/popamp/popamp

Files: *
Copyright: Popamp contributors
License: MIT or Apache-2.0
 See LICENSE-MIT and LICENSE-APACHE in this directory, and THIRD_PARTY.md
 for third-party credits. The Apache licence is also in
 /usr/share/common-licenses/Apache-2.0.
EOF

# Shared libraries are loaded at run time (dlopen), so dpkg-shlibdeps cannot
# see them: ALSA for sound, X11 and xkbcommon for windows and keys.
mkdir -p "$pkg/DEBIAN"
cat > "$pkg/DEBIAN/control" <<EOF
Package: popamp
Version: $version
Section: sound
Priority: optional
Architecture: $debarch
Maintainer: Gunyakov <web@gunyakov.pro>
Installed-Size: $(du -sk "$pkg/usr" | cut -f1)
Homepage: https://github.com/popamp/popamp
Depends: libc6 (>= $glibc), libasound2t64 | libasound2, libx11-6, libx11-xcb1, libxcursor1, libxi6, libxkbcommon0, libxkbcommon-x11-0, fontconfig
Recommends: zenity | kdialog, fonts-dejavu-core
Suggests: yt-dlp, ffmpeg
Description: Winamp-style skinnable audio player
 Popamp is an audio player in the style of the classic Winamp 2: main
 window, equalizer and playlist editor that dock together, with support
 for classic .wsz skins. It plays MP3, Ogg Vorbis, FLAC, WAV, AIFF,
 AAC/M4A and internet radio streams.
EOF

mkdir -p "$outdir"
deb="$outdir/popamp-${version}-linux-${debarch}.deb"
dpkg-deb -Zgzip --root-owner-group --build "$pkg" "$deb" >/dev/null
echo "Built $deb"
