#!/usr/bin/env bash
# Builds the `mouser` CLI and places it where Tauri's externalBin expects it:
# apps/mouser-app/binaries/mouser-<target-triple>[.exe].
# Usage: build-sidecar.sh <triple>            (single target)
#        build-sidecar.sh universal-apple-darwin   (arm64 + x86_64, lipo'd)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
out="$here/../binaries"
mkdir -p "$out"
triple="${1:?target triple required}"

build() { (cd "$root" && cargo build --release --locked -p mouser-cli --target "$1"); }

if [ "$triple" = "universal-apple-darwin" ]; then
  build aarch64-apple-darwin
  build x86_64-apple-darwin
  lipo -create -output "$out/mouser-universal-apple-darwin" \
    "$root/target/aarch64-apple-darwin/release/mouser" \
    "$root/target/x86_64-apple-darwin/release/mouser"
else
  build "$triple"
  case "$triple" in
    *windows*) ext=.exe ;;
    *) ext= ;;
  esac
  cp "$root/target/$triple/release/mouser$ext" "$out/mouser-$triple$ext"
fi
