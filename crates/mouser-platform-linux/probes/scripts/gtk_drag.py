#!/usr/bin/env python3
"""Fullscreen GTK4 window that is a drag source, to test InputCapture while a drag is in progress.
Prints drag-begin / drag-end (with whether the drag was cancelled) and a `pointer-leave` line. Quits after N secs.
usage: gtk_drag.py SECS"""
import sys, gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, Gdk, GLib
app = Gtk.Application(application_id="org.example.MouserDragProbe")
def activate(app):
    win = Gtk.ApplicationWindow(application=app)
    label = Gtk.Label(label="drag me")
    src = Gtk.DragSource()
    src.set_actions(Gdk.DragAction.COPY)
    src.connect("prepare", lambda s, x, y: Gdk.ContentProvider.new_for_value("probe-drag-payload"))
    src.connect("drag-begin", lambda s, d: print("gtk_drag: drag-begin", flush=True))
    src.connect("drag-end", lambda s, d, delete: print("gtk_drag: drag-end", flush=True))
    src.connect("drag-cancel", lambda s, d, reason: print("gtk_drag: drag-cancel", reason, flush=True) or False)
    label.add_controller(src)
    win.set_child(label)
    win.fullscreen()
    win.present()
    print("gtk_drag: ready", flush=True)
    GLib.timeout_add_seconds(int(sys.argv[1]), lambda: app.quit())
app.connect("activate", activate)
app.run([])
