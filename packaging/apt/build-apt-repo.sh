#!/bin/sh
# Builds Popamp's apt repository from the release's .deb files: a signed
# "stable" suite with both packages for amd64 and arm64, the public key
# (popamp.gpg) and a sources file (popamp.sources) that points at <url>.
# It holds what it is given, so each release replaces the whole repository.
# Usage: packaging/apt/build-apt-repo.sh <deb dir> <output dir> <url> [gpg key id]
#   The key, with its secret part, must be in gpg's keyring; without a key
#   id, gpg's default key signs.
# Needs: apt-ftparchive (apt-utils), dpkg-deb, gpg.
set -eu

if [ $# -lt 3 ]; then
    echo "usage: $0 <deb dir> <output dir> <url> [gpg key id]" >&2
    exit 1
fi
debs=$(cd "$1" && pwd)
out=$2
url=${3%/}
key=${4:-}
suite=stable
archs="amd64 arm64"

set -- "$debs"/*.deb
[ -f "$1" ] || { echo "error: no .deb files in $debs" >&2; exit 1; }

rm -rf "$out"
mkdir -p "$out/pool/main"
# apt-ftparchive --arch picks files by Debian's name_version_arch.deb.
for deb in "$debs"/*.deb; do
    name=$(dpkg-deb -f "$deb" Package)_$(dpkg-deb -f "$deb" Version)_$(dpkg-deb -f "$deb" Architecture).deb
    cp "$deb" "$out/pool/main/$name"
done
out=$(cd "$out" && pwd)
cd "$out"

for arch in $archs; do
    dir=dists/$suite/main/binary-$arch
    mkdir -p "$dir"
    apt-ftparchive --arch "$arch" packages pool > "$dir/Packages"
    gzip -9n < "$dir/Packages" > "$dir/Packages.gz"
done

apt-ftparchive \
    -o "APT::FTPArchive::Release::Origin=Popamp" \
    -o "APT::FTPArchive::Release::Label=Popamp" \
    -o "APT::FTPArchive::Release::Suite=$suite" \
    -o "APT::FTPArchive::Release::Codename=$suite" \
    -o "APT::FTPArchive::Release::Architectures=$archs" \
    -o "APT::FTPArchive::Release::Components=main" \
    release "dists/$suite" > "dists/$suite/Release.tmp"
mv "dists/$suite/Release.tmp" "dists/$suite/Release"

sign() { gpg --batch --yes ${key:+--local-user "$key"} "$@"; }
sign --clearsign -o "dists/$suite/InRelease" "dists/$suite/Release"
sign --armor --detach-sign -o "dists/$suite/Release.gpg" "dists/$suite/Release"
gpg --batch --yes --export ${key:+"$key"} > popamp.gpg
[ -s popamp.gpg ] || { echo "error: gpg exported no public key" >&2; exit 1; }

cat > popamp.sources <<EOF
Types: deb
URIs: $url
Suites: $suite
Components: main
Signed-By: /etc/apt/keyrings/popamp.gpg
EOF

for arch in $archs; do
    printf '%s: ' "$arch"
    grep '^Package:' "dists/$suite/main/binary-$arch/Packages" | cut -d' ' -f2 | tr '\n' ' '
    echo
done
