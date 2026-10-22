#!/bin/bash
# Real Xorg (dummy video driver) on :98 so that input comes from real libinput devices, which we create with uinput.
sudo pkill -x Xorg; sleep 0.5
sudo modprobe uinput; sudo chmod 666 /dev/uinput
sudo Xorg :98 -config "$(dirname "$(readlink -f "$0")")/xorg-dummy.conf" -ac -noreset -novtswitch -sharevts -logfile /tmp/xorg98.log >/tmp/xorg98.out 2>&1 &
for i in $(seq 1 20); do DISPLAY=:98 xdpyinfo >/dev/null 2>&1 && break; sleep 0.5; done
DISPLAY=:98 xdpyinfo | grep -E "version number|vendor string|dimensions"
