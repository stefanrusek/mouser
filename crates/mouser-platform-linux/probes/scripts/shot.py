#!/usr/bin/env python3
"""Screenshot of the headless GNOME session via mutter's ScreenCast D-Bus API + PipeWire + GStreamer.
usage: shot.py out.png"""
import sys, subprocess, time, dbus, dbus.mainloop.glib
from gi.repository import GLib
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
sc = dbus.Interface(bus.get_object("org.gnome.Mutter.ScreenCast", "/org/gnome/Mutter/ScreenCast"), "org.gnome.Mutter.ScreenCast")
sp = sc.CreateSession({})
sess = dbus.Interface(bus.get_object("org.gnome.Mutter.ScreenCast", sp), "org.gnome.Mutter.ScreenCast.Session")
stp = sess.RecordMonitor("Meta-0", {"cursor-mode": dbus.UInt32(1)})
st = bus.get_object("org.gnome.Mutter.ScreenCast", stp)
node = []
loop = GLib.MainLoop()
def on_added(n):
    node.append(int(n)); loop.quit()
st.connect_to_signal("PipeWireStreamAdded", on_added, dbus_interface="org.gnome.Mutter.ScreenCast.Stream")
sess.Start()
GLib.timeout_add_seconds(10, loop.quit)
loop.run()
if not node:
    sys.exit("no stream")
time.sleep(0.5)
subprocess.run(["gst-launch-1.0", "-q", "pipewiresrc", f"path={node[0]}", "num-buffers=3", "!", "videoconvert", "!", "pngenc", "snapshot=true", "!", "filesink", f"location={sys.argv[1]}"], check=True)
sess.Stop()
