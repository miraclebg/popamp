#!/bin/sh
# Sends a version of Popamp to its openSUSE Build Service package, which
# builds the rpms and publishes them in its repositories. Makes the two
# source tarballs (the project at <ref>, and its crates from cargo vendor,
# since OBS builds offline), puts them with the spec into an osc checkout
# and commits it. The tarballs are reproducible, so sending the same ref
# twice changes nothing.
# Usage: packaging/obs/publish.sh <osc checkout of the package> [git ref, default HEAD]
#   e.g. osc checkout home:GoodZon:popamp popamp   (once)
#        packaging/obs/publish.sh home:GoodZon:popamp/popamp v0.5.0
# Needs: git, cargo, xz, osc logged in to the project.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
checkout=$(cd "$1" && pwd)
ref=${2:-HEAD}

version=$(git -C "$root" show "$ref:Cargo.toml" | grep -m1 '^version' | sed 's/.*"\(.*\)".*/\1/')
name=popamp-$version
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

git -C "$root" archive --format=tar --prefix="$name/" "$ref" > "$work/src.tar"
tar -x -C "$work" -f "$work/src.tar"
(cd "$work/$name" && cargo vendor --quiet --locked --versioned-dirs vendor > cargo-config.toml)

rm -f "$checkout"/popamp-*.tar.xz
xz -T0 -6 < "$work/src.tar" > "$checkout/$name.tar.xz"
tar -c -C "$work" --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner \
    "$name/vendor" "$name/cargo-config.toml" | xz -T0 -6 > "$checkout/$name-vendor.tar.xz"
cp "$here/_multibuild" "$here/_constraints" "$checkout/"
sed "s/^Version:.*/Version:        $version/" "$here/popamp.spec" > "$checkout/popamp.spec"

cd "$checkout"
osc addremove
osc commit -m "Popamp $version ($(git -C "$root" rev-parse --short "$ref"))"
