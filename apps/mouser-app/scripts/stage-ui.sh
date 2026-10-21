#!/usr/bin/env bash
# Copies only the shippable UI files (no node_modules, tests or fixtures) to
# apps/mouser-app/dist-release, which tauri.release.conf.json uses as frontendDist.
set -euo pipefail
app="$(cd "$(dirname "$0")/.." && pwd)"
rm -rf "$app/dist-release"
mkdir -p "$app/dist-release"
for f in "$app"/ui/*.html "$app"/ui/*.css "$app"/ui/*.js; do
  case "$(basename "$f")" in
    *.test.js | test_helpers.js | status_fixture.js) ;;
    *) cp "$f" "$app/dist-release/" ;;
  esac
done
