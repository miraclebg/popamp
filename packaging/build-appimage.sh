#!/bin/sh
# Builds a Linamp .AppImage from an already-compiled release binary.
# Usage: packaging/build-appimage.sh [target/release/linamp] [output-dir]
# Needs: appimagetool (https://github.com/AppImage/appimagetool) and strip.
# Set APPIMAGE_RUNTIME to a runtime file to stop appimagetool downloading one.
# Libraries are not bundled: the AppImage uses the system's ALSA, X11 and
# xkbcommon, like the .deb (see build-deb.sh for the list).
# Adapted from Ferramp's build-appimage.sh (see THIRD_PARTY.md).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
bin=${1:-"$root/target/release/linamp"}
outdir=${2:-"$root/dist"}

if [ ! -x "$bin" ]; then
    echo "error: binary not found or not executable: $bin" >&2
    echo "hint: run 'cargo build --release -p linamp' first" >&2
    exit 1
fi
command -v appimagetool >/dev/null 2>&1 || {
    echo "error: appimagetool not found (https://github.com/AppImage/appimagetool)" >&2
    exit 1
}

version=$(grep -m1 '^version' "$root/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
case "$(uname -m)" in
    x86_64) arch=x86_64 ;;
    aarch64|arm64) arch=aarch64 ;;
    *) echo "error: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT INT TERM
appdir="$workdir/Linamp.AppDir"

install -Dm755 -s "$bin" "$appdir/usr/bin/linamp"
install -Dm644 "$here/linamp.desktop" "$appdir/usr/share/applications/linamp.desktop"
install -Dm644 "$here/linamp.metainfo.xml" "$appdir/usr/share/metainfo/linamp.appdata.xml"
install -Dm644 "$root/assets/logo/linamp.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/linamp.svg"
for n in 16 24 32 48 64 128 256; do
    install -Dm644 "$root/assets/logo/png/linamp-$n.png" "$appdir/usr/share/icons/hicolor/${n}x${n}/apps/linamp.png"
done
for f in README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md; do
    install -Dm644 "$root/$f" "$appdir/usr/share/doc/linamp/$f"
done
# appimagetool takes the desktop file and icon from the AppDir's top level.
install -m644 "$here/linamp.desktop" "$appdir/linamp.desktop"
install -m644 "$root/assets/logo/png/linamp-256.png" "$appdir/linamp.png"

cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/linamp" "$@"
EOF
chmod +x "$appdir/AppRun"

mkdir -p "$outdir"
out="$outdir/Linamp-${version}-linux-${arch}.AppImage"
set --
[ -n "${APPIMAGE_RUNTIME:-}" ] && set -- --runtime-file "$APPIMAGE_RUNTIME"
VERSION="$version" ARCH="$arch" appimagetool "$@" "$appdir" "$out"
echo "Built $out"
