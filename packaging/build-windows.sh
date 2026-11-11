#!/bin/sh
# Builds Popamp's Windows downloads from an already-compiled release
# popamp.exe: an installer (Inno Setup) and a portable zip. Popamp-Plus-…
# for one built with --features plus (its /VERSION says which).
# The zip holds one folder: the exe, its papers and an empty popamp.ini,
# which keeps the settings in that folder instead of %APPDATA%.
# Usage: packaging/build-windows.sh [target/release/popamp.exe] [output-dir] [libs-dir]
# libs-dir (default dist/libs) holds what packaging/windows/fetch-libs.sh
# fetched: the Opus and libopenmpt DLLs and the MIDI SoundFont.
# Needs: Git Bash on Windows, 7z (7-Zip) and Inno Setup 6 (ISCC.exe).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
bin=${1:-"$root/target/release/popamp.exe"}
outdir=${2:-"$root/dist"}
libs=${3:-"$root/dist/libs"}

if [ ! -f "$bin" ]; then
    echo "error: binary not found: $bin" >&2
    echo "hint: run 'cargo build --release -p popamp' first" >&2
    exit 1
fi
[ -f "$libs/opus.dll" ] && [ -f "$libs/libopenmpt.dll" ] && [ -f "$libs/GeneralUser-GS.sf2" ] || {
    echo "error: $libs lacks the DLLs or the SoundFont" >&2
    echo "hint: run 'sh packaging/windows/fetch-libs.sh $libs' first" >&2
    exit 1
}
command -v 7z >/dev/null 2>&1 || {
    echo "error: 7z not found (https://www.7-zip.org)" >&2
    exit 1
}
iscc=$(command -v iscc 2>/dev/null || echo "/c/Program Files (x86)/Inno Setup 6/ISCC.exe")
[ -f "$iscc" ] || {
    echo "error: Inno Setup's ISCC.exe not found (https://jrsoftware.org/isinfo.php)" >&2
    exit 1
}
# Git Bash would turn arguments such as /VERSION into paths under its own
# folder; the Windows programs here take them as they are.
export MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'

version=$(grep -m1 '^version' "$root/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
# "popamp 0.5.0" or "popamp-plus 0.5.0"; Windows may end the line with \r.
set -- $("$bin" /VERSION | tr -d '\r')
if [ "${2:-}" != "$version" ]; then
    echo "error: $bin is version ${2:-?}, Cargo.toml says $version" >&2
    exit 1
fi
case "$1" in
    popamp) name=Popamp; title=Popamp ;;
    popamp-plus) name=Popamp-Plus; title="Popamp Plus" ;;
    *) echo "error: $bin says it is $1, not popamp or popamp-plus" >&2; exit 1 ;;
esac

mkdir -p "$outdir"
outdir=$(cd "$outdir" && pwd)
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT INT TERM
folder="${name}-${version}-windows-x86_64"
mkdir "$workdir/$folder"
cp "$bin" "$workdir/$folder/popamp.exe"
for f in README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md; do
    cp "$root/$f" "$workdir/$folder/$f"
done
cp -r "$libs/." "$workdir/$folder/"

"$iscc" -Q "-DVersion=$version" "-DTitle=$title" "-DSource=$(cygpath -w "$workdir/$folder")" \
    "-DOutputName=$folder-setup" "-DOutputDir=$(cygpath -w "$outdir")" "$(cygpath -w "$here/windows/popamp.iss")"
echo "Built $outdir/$folder-setup.exe"

: > "$workdir/$folder/popamp.ini"
zip="$outdir/$folder-portable.zip"
rm -f "$zip"
# 7z is a Windows program: it reads /d/a/… as D:\d\a\…, so give it
# the Windows path.
(cd "$workdir" && 7z a -tzip -bso0 "$(cygpath -w "$zip")" "$folder")
[ -f "$zip" ] || { echo "error: 7z did not write $zip" >&2; exit 1; }
echo "Built $zip"
