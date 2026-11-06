#!/bin/sh
# Renders hero.html to hero.png, 2560×1440, with a headless Firefox of its
# own profile (a running Firefox is left alone). The screenshots it shows
# are in ../screenshots; retake them with popamp-website's tools/shoot.sh.
# Usage: assets/readme/make-hero.sh
# Needs: firefox, and Inter and JetBrains Mono installed as system fonts.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
profile=$(mktemp -d)
trap 'rm -rf "$profile"' EXIT INT TERM

firefox --headless --no-remote --profile "$profile" --window-size=2560,1440 \
    --screenshot "$here/hero.png" "file://$here/hero.html" >/dev/null 2>&1
echo "Built $here/hero.png"
