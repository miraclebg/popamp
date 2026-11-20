#!/bin/sh
# Builds a Popamp .AppImage from an already-compiled release binary;
# Popamp-Plus-… for a binary built with --features plus (its /VERSION
# says which).
# Usage: packaging/build-appimage.sh [target/release/popamp] [output-dir]
# Needs: appimagetool (https://github.com/AppImage/appimagetool), strip,
# readelf, patchelf, and the libraries in $bundle installed (on Ubuntu:
# libxkbcommon-x11-0 libxcursor1 libxi6 libopus0 libopenmpt0).
# Set APPIMAGE_RUNTIME to a runtime file to stop appimagetool downloading one.
# Adapted from Ferramp's build-appimage.sh (see THIRD_PARTY.md).
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
for tool in appimagetool readelf patchelf; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "error: $tool not found" >&2
        exit 1
    }
done

# The libraries Popamp loads while it runs and a desktop may lack (even
# Ubuntu with X11 can miss xkbcommon-x11), bundled with what they need in
# turn. What every desktop has, or must match the system's sound, display
# server and drivers, stays the system's: the C and C++ runtimes, ALSA,
# X11/xcb, Wayland and GL (they are never asked for here).
bundle="libxkbcommon.so.0 libxkbcommon-x11.so.0 libXcursor.so.1 libXi.so.6 libopus.so.0 libopenmpt.so.0"
system="ld-linux-x86-64.so.2 ld-linux-aarch64.so.1 libc.so.6 libm.so.6 libdl.so.2
    libpthread.so.0 librt.so.1 libgcc_s.so.1 libstdc++.so.6 libz.so.1 libasound.so.2
    libX11.so.6 libX11-xcb.so.1 libxcb.so.1 libXau.so.6 libXdmcp.so.6 libbsd.so.0
    libmd.so.0 libwayland-client.so.0"

version=$(grep -m1 '^version' "$root/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
# "popamp 0.5.0" or "popamp-plus 0.5.0".
set -- $("$bin" /VERSION)
if [ "${2:-}" != "$version" ]; then
    echo "error: $bin is version ${2:-?}, Cargo.toml says $version" >&2
    exit 1
fi
case "$1" in
    popamp) name=Popamp ;;
    popamp-plus) name=Popamp-Plus ;;
    *) echo "error: $bin says it is $1, not popamp or popamp-plus" >&2; exit 1 ;;
esac
case "$(uname -m)" in
    x86_64) arch=x86_64 ;;
    aarch64|arm64) arch=aarch64 ;;
    *) echo "error: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT INT TERM
appdir="$workdir/Popamp.AppDir"

install -Dm755 -s "$bin" "$appdir/usr/bin/popamp"

# Copies each library of $bundle and what it needs, and the licence of
# its package where dpkg knows it.
libdir="$appdir/usr/lib"
licenses="$appdir/usr/share/doc/popamp/libraries"
mkdir -p "$libdir" "$licenses"
queue=$bundle
while [ -n "$queue" ]; do
    set -- $queue
    lib=$1
    shift
    queue=$*
    case " $(echo $system) " in *" $lib "*) continue ;; esac
    [ -e "$libdir/$lib" ] && continue
    src=
    for dir in "/usr/lib/$arch-linux-gnu" /usr/lib64 /usr/lib; do
        [ -e "$dir/$lib" ] && { src=$(readlink -f "$dir/$lib"); break; }
    done
    [ -n "$src" ] || { echo "error: $lib not found; install it to bundle it" >&2; exit 1; }
    install -m644 "$src" "$libdir/$lib"
    if pkg=$(dpkg -S "$src" 2>/dev/null || dpkg -S "/lib/${src#/usr/lib/}" 2>/dev/null); then
        pkg=${pkg%%:*}
        install -m644 "/usr/share/doc/$pkg/copyright" "$licenses/$pkg.copyright"
    fi
    queue="$queue $(readelf -dW "$src" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p')"
done
# Popamp asks for them by name, and the libraries for theirs; an RPATH
# (not a RUNPATH) in the program covers both, and leaves the dialog
# programs it starts with the system's own.
patchelf --force-rpath --set-rpath '$ORIGIN/../lib' "$appdir/usr/bin/popamp"
install -Dm644 "$here/popamp.desktop" "$appdir/usr/share/applications/popamp.desktop"
install -Dm644 "$here/popamp.metainfo.xml" "$appdir/usr/share/metainfo/popamp.appdata.xml"
install -Dm644 "$root/assets/logo/popamp.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/popamp.svg"
for n in 16 24 32 48 64 128 256; do
    install -Dm644 "$root/assets/logo/png/popamp-$n.png" "$appdir/usr/share/icons/hicolor/${n}x${n}/apps/popamp.png"
done
for f in README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md; do
    install -Dm644 "$root/$f" "$appdir/usr/share/doc/popamp/$f"
done
# appimagetool takes the desktop file and icon from the AppDir's top level.
install -m644 "$here/popamp.desktop" "$appdir/popamp.desktop"
install -m644 "$root/assets/logo/png/popamp-256.png" "$appdir/popamp.png"

cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/popamp" "$@"
EOF
chmod +x "$appdir/AppRun"

mkdir -p "$outdir"
out="$outdir/${name}-${version}-${arch}.AppImage"
set --
[ -n "${APPIMAGE_RUNTIME:-}" ] && set -- --runtime-file "$APPIMAGE_RUNTIME"
VERSION="$version" ARCH="$arch" appimagetool "$@" "$appdir" "$out"
echo "Built $out"
