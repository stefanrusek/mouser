#!/usr/bin/env python3
"""Drive KWin through its own EIS endpoint (org.kde.KWin.EIS.RemoteDesktop.connectToEIS), no portal and no dialog:
the headless-VM stand-in for a physical keyboard/mouse on Plasma. Hands the socket to `spike_portals eisfd`.
usage: kwin_eis.py ACTION...   (abs:X,Y rel:DX,DY click btndown btnup key:CODE sleep:S)"""
import os, sys, dbus
bus = dbus.SessionBus()
o = bus.get_object("org.kde.KWin", "/org/kde/KWin/EIS/RemoteDesktop")
fd, cookie = dbus.Interface(o, "org.kde.KWin.EIS.RemoteDesktop").connectToEIS(7)
n = fd.take()
os.set_inheritable(n, True)
probe = os.path.expanduser("~/target-probes/debug/spike_portals")
# Stay alive as a child's parent: KWin drops the EIS connection when the D-Bus caller leaves the bus.
import subprocess
sys.exit(subprocess.run([probe, "eisfd", str(n)] + sys.argv[1:], pass_fds=[n]).returncode)
