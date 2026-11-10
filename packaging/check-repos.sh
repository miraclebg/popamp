#!/bin/sh
# Installs Popamp from its published repositories in fresh containers,
# the way docs/building.md tells users to, and checks what the packages
# put on the system: the version, the files, the libraries, the skin MIME
# type, a headless render of the windows, and a clean removal.
# Usage: packaging/check-repos.sh [distro ...]   (default: all of them)
#   distros: tumbleweed leap16 fedora43 fedora44 debian12 debian13
#            ubuntu2204 ubuntu2404
# Needs: docker (or podman as docker) and the network.
set -eu

all="tumbleweed leap16 fedora43 fedora44 debian12 debian13 ubuntu2204 ubuntu2404"
obs=https://download.opensuse.org/repositories/home:/GoodZon:/popamp
logs=${LOGS:-$(mktemp -d)}

image() {
    case $1 in
        tumbleweed) echo opensuse/tumbleweed:latest ;;
        leap16) echo opensuse/leap:16.0 ;;
        fedora43) echo fedora:43 ;;
        fedora44) echo fedora:44 ;;
        debian12) echo debian:12 ;;
        debian13) echo debian:13 ;;
        ubuntu2204) echo ubuntu:22.04 ;;
        ubuntu2404) echo ubuntu:24.04 ;;
        *) echo "unknown distro: $1" >&2; exit 2 ;;
    esac
}

# What a desktop has before Popamp: the MIME database tools. Then the
# commands from docs/building.md, without sudo, answering yes.
setup() {
    case $1 in
        tumbleweed | leap16)
            [ "$1" = leap16 ] && repo=openSUSE_Leap_16.0 || repo=openSUSE_Tumbleweed
            cat <<EOF
zypper -n install shared-mime-info >/dev/null
zypper -n addrepo $obs/$repo/home:GoodZon:popamp.repo
zypper -n --gpg-auto-import-keys refresh >/dev/null
pkg_install() { zypper -n install "\$1"; }
pkg_remove() { zypper -n remove "\$1"; }
EOF
            ;;
        fedora*)
            cat <<EOF
dnf -y install shared-mime-info >/dev/null
dnf config-manager addrepo --from-repofile=$obs/Fedora_${1#fedora}/home:GoodZon:popamp.repo
pkg_install() { dnf -y install "\$1"; }
pkg_remove() { dnf -y remove "\$1"; }
EOF
            ;;
        debian* | ubuntu*)
            cat <<'EOF'
export DEBIAN_FRONTEND=noninteractive
apt-get update -q >/dev/null && apt-get install -yq curl ca-certificates shared-mime-info >/dev/null
mkdir -p /etc/apt/keyrings
curl -fsSLo /etc/apt/keyrings/popamp.gpg https://apt.popamp.app/popamp.gpg
curl -fsSLo /etc/apt/sources.list.d/popamp.sources https://apt.popamp.app/popamp.sources
apt-get update -q >/dev/null
pkg_install() { apt-get install -yq "$1"; }
pkg_remove() { apt-get purge -yq "$1"; }
EOF
            ;;
    esac
}

# The checks, the same for each package and distro.
checks() {
    cat <<'EOF'
fail() { echo "FAIL $pkg: $*"; failed=1; bad=1; }
failed=0
for pkg in popamp popamp-plus; do
    bad=0
    pkg_install $pkg >/tmp/install.log 2>&1 || { tail -20 /tmp/install.log; fail "does not install"; continue; }
    version=$(popamp /VERSION 2>&1) || fail "/VERSION: $version"
    case $pkg in
        popamp-plus) echo "$version" | grep -qi "plus" || fail "not the Plus build: $version" ;;
        *) echo "$version" | grep -qi "plus" && fail "the Plus build: $version" ;;
    esac
    for f in /usr/share/applications/popamp.desktop /usr/share/metainfo/popamp.metainfo.xml \
        /usr/share/mime/packages/popamp-skin.xml /usr/share/icons/hicolor/scalable/apps/popamp.svg \
        /usr/share/icons/hicolor/32x32/apps/popamp.png; do
        [ -s "$f" ] || fail "missing $f"
    done
    missing=$(ldd /usr/bin/popamp | grep "not found" || true)
    [ -z "$missing" ] || fail "libraries not found: $missing"
    grep -q "application/x-popamp-skin" /usr/share/mime/globs2 || fail "the skin type is not in the MIME database"
    rm -rf /tmp/render && popamp /RENDER=/tmp/render >/tmp/render.log 2>&1 || { cat /tmp/render.log; fail "render"; }
    # 0.7.0 draws 16 window states, later versions more.
    # Plain sh: the openSUSE images have no find.
    pngs=0
    for f in /tmp/render/*.png; do [ -s "$f" ] && pngs=$((pngs + 1)); done
    [ "$pngs" -ge 16 ] || fail "rendered $pngs windows, not 16 or more"
    pkg_remove $pkg >/tmp/remove.log 2>&1 || { tail -5 /tmp/remove.log; fail "does not remove"; }
    left=$(ls /usr/bin/popamp /usr/share/applications/popamp.desktop 2>/dev/null || true)
    [ -z "$left" ] || fail "left after removal: $left"
    grep -q "application/x-popamp-skin" /usr/share/mime/globs2 && fail "the skin type stays in the MIME database"
    [ $bad = 1 ] || echo "ok $pkg: $version"
done
exit $failed
EOF
}

status=0
for distro in ${*:-$all}; do
    log=$logs/$distro.log
    if { setup "$distro"; checks; } | docker run --rm -i "$(image "$distro")" sh >"$log" 2>&1; then
        echo "ok   $distro: $(grep '^ok ' "$log" | sed 's/^ok //' | paste -sd ';' -)"
    else
        echo "FAIL $distro (log: $log)"
        grep '^FAIL ' "$log" | sed 's/^/     /'
        status=1
    fi
done
exit $status
