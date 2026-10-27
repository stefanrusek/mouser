#!/usr/bin/env bash
# Copies only the shippable UI files (no node_modules, tests or fixtures) to
# apps/mouser-app/dist-release, which tauri.release.conf.json uses as frontendDist,
# and the repository's troubleshooting guide to dist-release/help/ for the Help section.
set -euo pipefail
app="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$app/../.." && pwd)"
rm -rf "$app/dist-release"
mkdir -p "$app/dist-release/sections" "$app/dist-release/help"
shippable() {
  case "$(basename "$1")" in
    *.test.js | test_helpers.js | status_fixture.js) return 1 ;;
    *) return 0 ;;
  esac
}
for f in "$app"/ui/*.html "$app"/ui/*.css "$app"/ui/*.js; do
  if shippable "$f"; then cp "$f" "$app/dist-release/"; fi
done
for f in "$app"/ui/sections/*.js; do
  if shippable "$f"; then cp "$f" "$app/dist-release/sections/"; fi
done
cp "$repo/docs/troubleshooting.md" "$app/dist-release/help/troubleshooting.md"
