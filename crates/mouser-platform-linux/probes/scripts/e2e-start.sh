#!/bin/bash
# Throwaway e2e harness: a headless X11 session running the real mouser-app.
# Usage: e2e-start.sh <target-dir>   (inside a Lima VM; logs in ~/e2e/)
set -eu
T=${1:-$HOME/target}
mkdir -p ~/e2e ~/e2e/run ~/e2e/home; chmod 700 ~/e2e/run
pkill -f "Xvfb :99" || true; pkill -f "debug/mouser-app" || true
pkill -f gnome-keyring || true; pkill -f xdg-desktop-portal || true; pkill -f xdg-document-portal || true
sleep 1
rm -rf ~/e2e/run ~/e2e/home; mkdir -p ~/e2e/run ~/e2e/home; chmod 700 ~/e2e/run
Xvfb :99 -screen 0 1280x800x24 >~/e2e/xvfb.log 2>&1 &
sleep 2
export DISPLAY=:99 XDG_RUNTIME_DIR=$HOME/e2e/run XDG_SESSION_TYPE=x11 XDG_CONFIG_HOME=$HOME/e2e/home/.config XDG_DATA_HOME=$HOME/e2e/home/.local/share
unset WAYLAND_DISPLAY
nohup dbus-run-session -- "$T/debug/mouser-app" >~/e2e/app.log 2>&1 &
sleep 4
tail -5 ~/e2e/app.log
