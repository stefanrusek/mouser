#!/bin/bash
# T428 X11 half: grab failures with an "open menu" (another client holding the grab) and XFixes barriers, under Xvfb.
. "$(dirname "$0")/env.sh"
X=~/target-probes/debug/spike_lock_drag_x11
pkill Xvfb; sleep 0.5
Xvfb :99 -screen 0 1920x1080x24 +extension XFIXES +extension XTEST +extension RANDR >/tmp/xvfb.log 2>&1 &
sleep 1
export DISPLAY=:99; unset WAYLAND_DISPLAY
echo "== grab with nobody holding it"; $X grab 2
echo "== grab while another client holds the grab for 3 s (stand-in for an open menu)"
$X holder 3 & HP=$!; sleep 0.5; $X grab 10; wait $HP
echo "== grab while a GTK3 popup menu is open (a real menu)"
GDK_BACKEND=x11 python3 $(dirname "$0")/gtk3_menu.py 8 & MP=$!; sleep 2.5; xdotool mousemove 100 100 click 3; sleep 0.8; $X grab 15; wait $MP
echo "== barriers"; $X barrier
pkill Xvfb
