#!/usr/bin/env python3
"""GTK3 window that pops up a context menu on right click (a real X11 menu grabs pointer+keyboard).
usage: gtk3_menu.py SECS   (prints menu-open / menu-closed)"""
import sys, gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, GLib
win = Gtk.Window(title="menu probe"); win.set_default_size(1920, 1080); win.move(0, 0)
menu = Gtk.Menu()
for t in ("one", "two", "three"):
    menu.append(Gtk.MenuItem(label=t))
menu.show_all()
menu.connect("map", lambda *_: print("gtk3_menu: menu-open", flush=True))
menu.connect("unmap", lambda *_: print("gtk3_menu: menu-closed", flush=True))
def press(w, ev):
    print('gtk3_menu: button', ev.button, flush=True)
    if ev.button == 3:
        menu.popup_at_pointer(ev)
win.add_events(1 << 8)  # BUTTON_PRESS_MASK
win.connect("button-press-event", press)
win.connect("destroy", Gtk.main_quit)
win.show_all()
GLib.timeout_add_seconds(int(sys.argv[1]), Gtk.main_quit)
Gtk.main()
