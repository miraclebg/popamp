#!/bin/sh
# Builds a Popamp .zip for Windows from an already-compiled release
# popamp.exe; Popamp-Plus-… for one built with --features plugins (its
# /VERSION says which). The zip holds one folder: the exe and its papers.
# Usage: packaging/build-zip.sh [target/release/popamp.exe] [output-dir]
# Needs: 7z (7-Zip), and a system that runs the exe (Windows, or Git Bash).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
bin=${1:-"$root/target/release/popamp.exe"}
outdir=${2:-"$root/dist"}

if [ ! -f "$bin" ]; then
    echo "error: binary not found: $bin" >&2
    echo "hint: run 'cargo build --release -p popamp' first" >&2
    exit 1
fi
command -v 7z >/dev/null 2>&1 || {
    echo "error: 7z not found (https://www.7-zip.org)" >&2
    exit 1
}

version=$(grep -m1 '^version' "$root/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
# "popamp 0.1.0" or "popamp-plus 0.1.0"; Windows may end the line with \r.
set -- $("$bin" /VERSION | tr -d '\r')
if [ "${2:-}" != "$version" ]; then
    echo "error: $bin is version ${2:-?}, Cargo.toml says $version" >&2
    exit 1
fi
case "$1" in
    popamp) name=Popamp ;;
    popamp-plus) name=Popamp-Plus ;;
    *) echo "error: $bin says it is $1, not popamp or popamp-plus" >&2; exit 1 ;;
esac

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT INT TERM
folder="${name}-${version}-windows-x86_64"
mkdir "$workdir/$folder"
cp "$bin" "$workdir/$folder/popamp.exe"
for f in README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md; do
    cp "$root/$f" "$workdir/$folder/$f"
done

mkdir -p "$outdir"
out="$(cd "$outdir" && pwd)/$folder.zip"
rm -f "$out"
(cd "$workdir" && 7z a -tzip -bso0 "$out" "$folder")
echo "Built $out"
