#!/bin/sh
# Puts what Popamp loads at run time on Windows into <dir>, to go beside
# popamp.exe: opus.dll (from vcpkg), libopenmpt.dll and its helpers (the
# official build), the GeneralUser GS SoundFont for MIDI, and their
# licenses under <dir>/licenses. Linux takes these from the system.
# Usage: packaging/windows/fetch-libs.sh <dir>
# Needs: Git Bash on Windows, curl, unzip, sha256sum and vcpkg
# (VCPKG_INSTALLATION_ROOT, as GitHub's Windows runners set it).
set -eu

dir=${1:?usage: fetch-libs.sh <dir>}

# Pinned downloads; a new version means a new hash here.
openmpt_version=0.8.10
openmpt_sha256=52481ca47d5386616adabd8048d0e7161f8abef9063afc9b241daca1e00251db
soundfont_commit=684543d5e5efaef08d02be50dcda8d552478fa60
soundfont_sha256=9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe

vcpkg_root=${VCPKG_INSTALLATION_ROOT:?set VCPKG_INSTALLATION_ROOT to the vcpkg folder}

mkdir -p "$dir/licenses"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

fetch() { # <url> <sha256> <file>
    curl -fsSL --retry 3 -o "$3" "$1"
    echo "$2  $3" | sha256sum -c --quiet -
}

fetch "https://lib.openmpt.org/files/libopenmpt/dev/libopenmpt-$openmpt_version+release.dev.windows.vs2022.zip" \
    "$openmpt_sha256" "$tmp/openmpt.zip"
unzip -q "$tmp/openmpt.zip" -d "$tmp/openmpt"
cp "$tmp/openmpt/bin/amd64/"*.dll "$dir/"
mkdir -p "$dir/licenses/libopenmpt"
cp "$tmp/openmpt/LICENSE.txt" "$tmp/openmpt/Licenses/"* "$dir/licenses/libopenmpt/"

raw="https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/$soundfont_commit"
fetch "$raw/GeneralUser-GS.sf2" "$soundfont_sha256" "$dir/GeneralUser-GS.sf2"
curl -fsSL --retry 3 -o "$dir/licenses/GeneralUser-GS.txt" "$raw/documentation/LICENSE.txt"

"$vcpkg_root/vcpkg" install opus:x64-windows >/dev/null
cp "$vcpkg_root/installed/x64-windows/bin/opus.dll" "$dir/"
cp "$vcpkg_root/installed/x64-windows/share/opus/copyright" "$dir/licenses/opus.txt"

ls "$dir"
