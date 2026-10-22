#!/usr/bin/env python3
"""A plain Wayland client (GTK4) that puts text on the clipboard after a delay, to test
SelectionOwnerChanged on a portal clipboard session. usage: gtk_clip.py DELAY_SECS TEXT"""
import sys, gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, GLib
delay, text = float(sys.argv[1]), sys.argv[2]
app = Gtk.Application(application_id="org.example.MouserClipProbe")
def activate(app):
    win = Gtk.ApplicationWindow(application=app, default_width=300, default_height=100)
    win.present()
    def put():
        win.get_clipboard().set(text)
        print("gtk_clip: set", text, flush=True)
        return False
    GLib.timeout_add(int(delay * 1000), put)
    GLib.timeout_add(int(delay * 1000) + 12000, lambda: app.quit())
app.connect("activate", activate)
app.run([])
