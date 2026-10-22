#!/bin/bash
# Headless KWin (Plasma) Wayland session as a transient systemd user unit. usage: kde-headless.sh start|stop
case "$1" in
start)
  mkdir -p ~/.config/systemd/user
  printf '[Unit]\nDescription=Headless graphical session\n' > ~/.config/systemd/user/graphical-session.target
  systemctl --user daemon-reload
  systemctl --user start graphical-session.target
  dbus-update-activation-environment --systemd XDG_CURRENT_DESKTOP=KDE XDG_SESSION_TYPE=wayland WAYLAND_DISPLAY=wayland-0
  systemctl --user start pipewire wireplumber
  systemd-run --user --unit=mouser-kwin --collect \
    -E XDG_SESSION_TYPE=wayland -E XDG_CURRENT_DESKTOP=KDE -E XDG_SESSION_DESKTOP=KDE \
    kwin_wayland --virtual --width 1920 --height 1080 --socket wayland-0 --no-lockscreen
  for i in $(seq 1 40); do
    gdbus call --session --dest org.kde.KWin --object-path /KWin --method org.kde.KWin.supportInformation >/dev/null 2>&1 && { echo kwin-up; break; }
    sleep 1
  done ;;
stop)
  systemctl --user stop mouser-kwin xdg-desktop-portal xdg-desktop-portal-kde 2>/dev/null || true ;;
esac
