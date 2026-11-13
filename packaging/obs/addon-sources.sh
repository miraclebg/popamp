#!/bin/sh
# Turns the .deb of a skin or plug-in (a release asset of skin-<name> or
# plugin-<id>, made by popamp/apt's bin/build-addon-deb) into the sources
# of its rpm: a tarball of the same files and a spec. So the rpm ships
# exactly what the deb does, under the same name, version and
# dependencies. The tarball is reproducible: the same deb gives the same
# bytes, so publishing it again changes nothing.
# Usage: packaging/obs/addon-sources.sh <deb> <out dir>
#   writes <out dir>/<package>.spec and <out dir>/<package>-<version>.tar.xz
# Needs: dpkg-deb, tar, xz.
set -eu

die() { echo "error: $*" >&2; exit 1; }

[ $# -eq 2 ] || die "usage: $0 <deb> <out dir>"
deb=$1
out=$2

field() { dpkg-deb -f "$deb" "$1"; }
package=$(field Package)
version=$(field Version)
homepage=$(field Homepage)
case $package in
    popamp-skin-* | popamp-plugin-*) ;;
    *) die "$deb: $package is not a skin or plug-in" ;;
esac
[ "$(field Architecture)" = all ] || die "$deb: not Architecture: all"

# "a (>> 1), b" -> one line per package, in rpm's words: "a > 1", "b".
rpm_deps() {
    tr ',' '\n' | sed 's/^ *//; s/ *$//; /^$/d; s/ *(\([<>=]*\) *\([^)]*\))/ \1 \2/; s/ >> / > /; s/ << / < /'
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM
tree=$work/$package-$version
dpkg-deb -x "$deb" "$tree"

# The add-on's folder, e.g. usr/share/popamp/skins/bar, and nothing else
# but its documents.
dirs=$(cd "$tree" && ls -d usr/share/popamp/*/*)
[ "$(echo "$dirs" | wc -l)" -eq 1 ] || die "$deb: not one add-on folder: $dirs"
case $dirs in
    usr/share/popamp/skins/* | usr/share/popamp/plugins/*) ;;
    *) die "$deb: $dirs is not a skin or plug-in folder" ;;
esac
kind=${dirs#usr/share/popamp/}
kind=${kind%/*}
others=$(cd "$tree" && find usr -type f ! -path "$dirs/*" ! -path "usr/share/doc/$package/*")
[ -z "$others" ] || die "$deb: files outside $dirs: $others"

# The licences go to the top, for %license. The Debian copyright file
# names the licence of the files: the usual MIT or Apache-2.0 says no more
# than LICENSE-*, any other (artwork by other authors) goes along as
# COPYRIGHT.
doc=$tree/usr/share/doc/$package
[ -f "$doc/copyright" ] || die "$deb: no copyright file"
license=$(sed -n 's/^License: *//p' "$doc/copyright" | head -1)
case $license in
    "") die "$deb: the copyright file names no licence" ;;
    "MIT or Apache-2.0") license="MIT OR Apache-2.0" ;;
    *) mv "$doc/copyright" "$tree/COPYRIGHT" ;;
esac
licenses=
for f in "$tree/COPYRIGHT" "$doc"/LICENSE-*; do
    [ -f "$f" ] || continue
    [ "${f%/*}" = "$tree" ] || mv "$f" "$tree/"
    licenses="$licenses ${f##*/}"
done
rm -rf "$tree/usr/share/doc"
[ -n "$licenses" ] || die "$deb: no LICENSE-* files"

mkdir -p "$out"
tar -c -C "$work" --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner \
    "$package-$version" | xz -6 > "$out/$package-$version.tar.xz"

summary=$(field Description | head -1)
description=$(field Description | sed '1d; s/^ //; s/^\.$//')
{
    cat <<EOF
# Made by packaging/obs/addon-sources.sh from ${package}_${version}_all.deb,
# the release asset of $homepage; edit that, not this.
Name:           $package
Version:        $version
Release:        0
Summary:        $summary
License:        $license
%if 0%{?suse_version}
Group:          Productivity/Multimedia/Sound/Players
%endif
URL:            $homepage
Source0:        %{name}-%{version}.tar.xz
BuildArch:      noarch
EOF
    field Depends | rpm_deps | sed 's/^/Requires:       /'
    field Recommends | rpm_deps | sed 's/^/Recommends:     /'
    cat <<EOF

%description
$description

%prep
%setup -q

%build

%install
mkdir -p %{buildroot}%{_datadir}/popamp/$kind
cp -a $dirs %{buildroot}%{_datadir}/popamp/$kind/

%files
%license$licenses
%dir %{_datadir}/popamp
%dir %{_datadir}/popamp/$kind
%{_datadir}/popamp/$kind/${dirs##*/}

%changelog
EOF
} > "$out/$package.spec"
echo "$out/$package.spec"
