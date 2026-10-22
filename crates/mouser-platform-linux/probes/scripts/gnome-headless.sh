#!/bin/bash
# Start (or stop) a headless GNOME Shell Wayland session as transient systemd user units.
# usage: gnome-headless.sh start|stop
set -e
case "$1" in
start)
  systemctl --user stop xdg-desktop-portal xdg-desktop-portal-gnome xdg-desktop-portal-gtk xdg-document-portal 2>/dev/null || true
  # No gnome-session in a headless VM, so graphical-session.target (RefuseManualStart upstream)
  # never activates and the GNOME portal units' Requisite= fails. Shadow it with a startable copy.
  rm -rf ~/.config/systemd/user/xdg-desktop-portal-*.service.d
  mkdir -p ~/.config/systemd/user
  printf '[Unit]\nDescription=Headless graphical session\n' > ~/.config/systemd/user/graphical-session.target
  systemctl --user daemon-reload
  systemctl --user start graphical-session.target
  dbus-update-activation-environment --systemd XDG_CURRENT_DESKTOP=GNOME XDG_SESSION_TYPE=wayland WAYLAND_DISPLAY=wayland-0
  systemd-run --user --unit=mouser-gnome-shell --collect \
    -E XDG_SESSION_TYPE=wayland -E XDG_CURRENT_DESKTOP=GNOME -E XDG_SESSION_DESKTOP=gnome \
    -E MUTTER_DEBUG_DUMMY_MODE_SPECS=1920x1080 \
    gnome-shell --headless --wayland --no-x11 --virtual-monitor 1920x1080
  for i in $(seq 1 60); do
    if gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1; then echo shell-up; break; fi
    sleep 1
  done
  ;;
stop)
  systemctl --user stop mouser-gnome-shell 2>/dev/null || true
  ;;
esac
