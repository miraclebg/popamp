#!/bin/sh
# Sends the skins and plug-ins to the openSUSE Build Service, which builds
# their rpms and publishes them in Popamp's repositories, next to popamp
# and popamp-plus. Each public repository skin-<name> or plugin-<id> of
# popamp whose latest release carries a .deb becomes the OBS package of
# the same name (popamp-skin-<name>, popamp-plugin-<id>), made from that
# deb by addon-sources.sh. A new add-on needs no change here: its first
# release is picked up on the next run. Sending the same release twice
# changes nothing.
# Usage: packaging/obs/publish-addons.sh <osc checkout of the project> [repo ...]
#   e.g. osc checkout home:GoodZon:popamp   (once)
#        packaging/obs/publish-addons.sh home:GoodZon:popamp
#        packaging/obs/publish-addons.sh home:GoodZon:popamp skin-steel
# Needs: gh, osc logged in to the project, dpkg-deb, tar, xz.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
project_dir=$(cd "$1" && pwd)
shift
project=$(cat "$project_dir/.osc/_project")
owner=popamp

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

repos=${*:-$(gh repo list "$owner" --visibility public --limit 200 --json name -q '.[].name' |
    grep -E '^(skin|plugin)-[a-z0-9][a-z0-9-]*$' | sort)}

xml_text() { sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g'; }

# Creates the OBS package if the project does not have it yet.
ensure_package() {
    osc meta pkg "$project" "$1" >/dev/null 2>&1 && return
    {
        echo "<package name=\"$1\" project=\"$project\">"
        echo "  <title>$(echo "$2" | xml_text)</title>"
        echo "  <description>$(echo "$3" | xml_text)</description>"
        echo "</package>"
    } | osc meta pkg "$project" "$1" -F -
    echo "created $project/$1"
}

for repo in $repos; do
    tag=$(gh release view -R "$owner/$repo" --json tagName -q .tagName 2>/dev/null) || {
        echo "skip $repo: no release"
        continue
    }
    mkdir -p "$work/$repo"
    gh release download "$tag" -R "$owner/$repo" -p '*_all.deb' -D "$work/$repo" 2>/dev/null || {
        echo "skip $repo $tag: no .deb in the release"
        continue
    }
    deb=$(ls "$work/$repo"/*_all.deb)
    [ "$(echo "$deb" | wc -l)" -eq 1 ] || { echo "error: $repo $tag: more than one .deb" >&2; exit 1; }
    package=$(dpkg-deb -f "$deb" Package)
    [ "$package" = "popamp-$repo" ] || { echo "error: $repo $tag ships $package" >&2; exit 1; }

    ensure_package "$package" "$(dpkg-deb -f "$deb" Description | head -1)" "$(dpkg-deb -f "$deb" Homepage)"
    (cd "$project_dir" && osc update "$package" >/dev/null 2>&1) ||
        (cd "$project_dir" && osc checkout "$package" >/dev/null)
    checkout=$project_dir/$package

    rm -f "$checkout/$package".spec "$checkout/$package"-*.tar.xz
    "$here/addon-sources.sh" "$deb" "$checkout" >/dev/null
    cd "$checkout"
    osc addremove >/dev/null
    if [ -n "$(osc status)" ]; then
        osc commit -m "$package $(dpkg-deb -f "$deb" Version) ($repo $tag)"
    else
        echo "same $package $tag"
    fi
    cd - >/dev/null
done
