#!/usr/bin/env python3
"""Synthetic input through mutter's own org.gnome.Mutter.RemoteDesktop (no portal, no dialog).
Stands in for a physical keyboard/mouse inside a headless VM.
usage: drive.py "wait 2" "key 15" "key 28" "move 3000 0" "click 272" ...
  key N   = press+release evdev keycode N     move DX DY = relative pointer motion (in 100 steps of DX/100)
  click N = press+release evdev button N       wait S     = sleep
  moveto X Y = absolute pointer position (clamp to bottom-right then travel; 1920x1080 only)
  keydown N / keyup N / btndown N / btnup N, scroll V (discrete steps)
"""
import sys, time, dbus

bus = dbus.SessionBus()
rd = dbus.Interface(bus.get_object("org.gnome.Mutter.RemoteDesktop", "/org/gnome/Mutter/RemoteDesktop"),
                    "org.gnome.Mutter.RemoteDesktop")
path = rd.CreateSession()
sess = dbus.Interface(bus.get_object("org.gnome.Mutter.RemoteDesktop", path),
                      "org.gnome.Mutter.RemoteDesktop.Session")
sess.Start()
for step in sys.argv[1:]:
    a = step.split()
    c = a[0]
    if c == "wait":
        time.sleep(float(a[1]))
    elif c == "key":
        sess.NotifyKeyboardKeycode(dbus.UInt32(int(a[1])), True); time.sleep(0.05)
        sess.NotifyKeyboardKeycode(dbus.UInt32(int(a[1])), False); time.sleep(0.1)
    elif c == "keydown":
        sess.NotifyKeyboardKeycode(dbus.UInt32(int(a[1])), True)
    elif c == "keyup":
        sess.NotifyKeyboardKeycode(dbus.UInt32(int(a[1])), False)
    elif c == "click":
        time.sleep(0.2)  # let the pointer settle/hover first: GNOME 50 ignores a click that arrives with the motion
        sess.NotifyPointerButton(dbus.Int32(int(a[1])), True); time.sleep(0.2)
        sess.NotifyPointerButton(dbus.Int32(int(a[1])), False); time.sleep(0.2)
    elif c == "btndown":
        sess.NotifyPointerButton(dbus.Int32(int(a[1])), True)
    elif c == "btnup":
        sess.NotifyPointerButton(dbus.Int32(int(a[1])), False)
    elif c == "move":
        dx, dy = float(a[1]), float(a[2])
        for _ in range(100):
            sess.NotifyPointerMotionRelative(dx / 100, dy / 100); time.sleep(0.005)
    elif c == "moveto":
        # absolute: clamp to the bottom-right corner (the top-left one is GNOME's Activities hot corner),
        # then travel back; assumes one 1920x1080 monitor
        for _ in range(100):
            sess.NotifyPointerMotionRelative(100.0, 100.0); time.sleep(0.002)
        x, y = float(a[1]) - 1919, float(a[2]) - 1079
        for _ in range(100):
            sess.NotifyPointerMotionRelative(x / 100, y / 100); time.sleep(0.002)
    elif c == "scroll":
        sess.NotifyPointerAxisDiscrete(dbus.UInt32(0), dbus.Int32(int(a[1])))
    else:
        raise SystemExit("bad step " + step)
sess.Stop()
