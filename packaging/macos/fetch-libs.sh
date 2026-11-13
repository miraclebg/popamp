#!/bin/sh
# Puts what Popamp.app carries into <dir>, for packaging/build-macos.sh:
# libopus and libopenmpt built from their source releases for macOS 12 and
# newer (Homebrew's builds ask for the newest macOS, so they cannot be
# shipped), the GeneralUser GS SoundFont for MIDI, and their licenses under
# <dir>/licenses. Each library's install name points into the bundle's
# Frameworks folder.
# Usage: packaging/macos/fetch-libs.sh <dir>
# Needs: the Xcode Command Line Tools, curl and shasum. ARCH (default
# arm64) picks the architecture.
set -eu

dir=${1:?usage: fetch-libs.sh <dir>}
arch=${ARCH:-arm64}
target=12.0

# Pinned downloads; a new version means a new hash here.
opus_version=1.5.2
opus_sha256=65c1d2f78b9f2fb20082c38cbe47c951ad5839345876e46941612ee87f9a7ce1
openmpt_version=0.8.10
openmpt_sha256=976d259288c40cd285771f77d09eff3d02514511d5677ecfcfb53be5013317e2
soundfont_commit=684543d5e5efaef08d02be50dcda8d552478fa60
soundfont_sha256=9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe

mkdir -p "$dir/licenses"
dir=$(cd "$dir" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

fetch() { # <url> <sha256> <file>
    curl -fsSL --retry 3 -o "$3" "$1"
    echo "$2  $3" | shasum -a 256 -c --quiet -
}

# Built for macOS 12 on $arch, and blind to Homebrew's libraries, so
# nothing but the system's own is linked.
export MACOSX_DEPLOYMENT_TARGET=$target
export CFLAGS="-arch $arch -mmacosx-version-min=$target -O2"
export CXXFLAGS="$CFLAGS"
export LDFLAGS="-arch $arch -mmacosx-version-min=$target"
# pkg-config sees only a description of the system's zlib.
mkdir -p "$tmp/pkgconfig"
printf 'Name: zlib\nDescription: the system zlib\nVersion: 1.2.12\nLibs: -lz\nCflags:\n' > "$tmp/pkgconfig/zlib.pc"
export PKG_CONFIG_LIBDIR="$tmp/pkgconfig"
jobs=$(sysctl -n hw.ncpu)

# Copies the built library <path> into $dir as <name>, named for the bundle.
take() { # <path> <name>
    cp "$1" "$dir/$2"
    chmod u+w "$dir/$2"
    install_name_tool -id "@executable_path/../Frameworks/$2" "$dir/$2"
}

fetch "https://downloads.xiph.org/releases/opus/opus-$opus_version.tar.gz" "$opus_sha256" "$tmp/opus.tar.gz"
tar -xzf "$tmp/opus.tar.gz" -C "$tmp"
(
    cd "$tmp/opus-$opus_version"
    ./configure --host="$arch-apple-darwin" --disable-static --disable-doc --disable-extra-programs >/dev/null
    make -j"$jobs" >/dev/null
)
take "$tmp/opus-$opus_version/.libs/libopus.0.dylib" libopus.0.dylib
cp "$tmp/opus-$opus_version/COPYING" "$dir/licenses/opus.txt"

# Without its optional decoders (mpg123, Ogg Vorbis: only for MO3 files
# with compressed samples) libopenmpt needs nothing beyond the system.
fetch "https://lib.openmpt.org/files/libopenmpt/src/libopenmpt-$openmpt_version+release.autotools.tar.gz" \
    "$openmpt_sha256" "$tmp/openmpt.tar.gz"
tar -xzf "$tmp/openmpt.tar.gz" -C "$tmp"
(
    cd "$tmp/libopenmpt-$openmpt_version+release.autotools"
    ./configure --host="$arch-apple-darwin" --disable-static \
        --disable-openmpt123 --disable-examples --disable-tests --disable-doxygen-doc \
        --without-mpg123 --without-ogg --without-vorbis --without-vorbisfile \
        --without-portaudio --without-portaudiocpp --without-sndfile --without-flac >/dev/null
    make -j"$jobs" >/dev/null
)
take "$(find "$tmp/libopenmpt-$openmpt_version+release.autotools" -name libopenmpt.0.dylib -type f | head -n 1)" libopenmpt.0.dylib
mkdir -p "$dir/licenses/libopenmpt"
cp "$tmp/libopenmpt-$openmpt_version+release.autotools/LICENSE" "$dir/licenses/libopenmpt/LICENSE.txt"

raw="https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/$soundfont_commit"
fetch "$raw/GeneralUser-GS.sf2" "$soundfont_sha256" "$dir/GeneralUser-GS.sf2"
curl -fsSL --retry 3 -o "$dir/licenses/GeneralUser-GS.txt" "$raw/documentation/LICENSE.txt"

ls "$dir"
