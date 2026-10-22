#!/usr/bin/env python3
"""Screenshot of headless KWin via org.kde.KWin.ScreenShot2 (needs the restricted-interface desktop file, see
install-kde-shot-desktop.sh). usage: kwin_shot.py out.png"""
import os, sys, struct, zlib, threading, dbus
bus = dbus.SessionBus()
ss = dbus.Interface(bus.get_object("org.kde.KWin", "/org/kde/KWin/ScreenShot2"), "org.kde.KWin.ScreenShot2")
r, w = os.pipe()
chunks = []
def reader():
    while True:
        b = os.read(r, 1 << 20)
        if not b: break
        chunks.append(b)
t = threading.Thread(target=reader); t.start()
res = ss.CaptureActiveScreen({"include-cursor": True, "native-resolution": True}, dbus.types.UnixFd(w))
os.close(w)
t.join()
data = b"".join(chunks)
width, height, stride = int(res["width"]), int(res["height"]), int(res["stride"])
rows = []
for y in range(height):
    row = data[y * stride:y * stride + width * 4]
    out = bytearray(width * 3)
    out[0::3] = row[2::4]; out[1::3] = row[1::4]; out[2::3] = row[0::4]   # BGRA -> RGB
    rows.append(b"\0" + bytes(out))
def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
