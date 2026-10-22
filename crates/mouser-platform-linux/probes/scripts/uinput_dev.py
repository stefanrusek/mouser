#!/usr/bin/env python3
"""Virtual mouse + keyboard through /dev/uinput (real kernel input devices: libinput/Xorg or a compositor sees
them like physical hardware). A daemon reading commands from a FIFO so the devices persist across calls.
  uinput_dev.py daemon            create devices, serve /tmp/uinput.fifo
  uinput_dev.py send CMD...       e.g.  send "move 100 0" "btn 272 1" "btn 272 0" "key 30 1" "key 30 0" "scroll 1" "sleep 0.2"
"""
import os, sys, time
from evdev import UInput, ecodes as e

FIFO = "/tmp/uinput.fifo"

def daemon():
    mouse = UInput({e.EV_REL: [e.REL_X, e.REL_Y, e.REL_WHEEL, e.REL_HWHEEL],
                    e.EV_KEY: [e.BTN_LEFT, e.BTN_RIGHT, e.BTN_MIDDLE]}, name="mouser-probe-mouse")
    kbd = UInput({e.EV_KEY: list(range(1, 100))}, name="mouser-probe-keyboard")
    if os.path.exists(FIFO):
        os.unlink(FIFO)
    os.mkfifo(FIFO)
    os.chmod(FIFO, 0o666)
    print("uinput_dev: ready", flush=True)
    while True:
        with open(FIFO) as f:
            for line in f:
                a = line.split()
                if not a:
                    continue
                if a[0] == "move":
                    dx, dy = int(a[1]), int(a[2])
                    # small steps like a real mouse (also keeps X's pointer accel predictable)
                    n = max(abs(dx), abs(dy), 1)
                    steps = max(1, n // 5)
                    for i in range(steps):
                        mouse.write(e.EV_REL, e.REL_X, dx // steps + (1 if i < dx % steps else 0) if dx >= 0 else -((-dx) // steps) )
                        mouse.write(e.EV_REL, e.REL_Y, dy // steps + (1 if i < dy % steps else 0) if dy >= 0 else -((-dy) // steps))
                        mouse.syn(); time.sleep(0.002)
                elif a[0] == "btn":
                    mouse.write(e.EV_KEY, int(a[1]), int(a[2])); mouse.syn()
                elif a[0] == "key":
                    kbd.write(e.EV_KEY, int(a[1]), int(a[2])); kbd.syn()
                elif a[0] == "scroll":
                    mouse.write(e.EV_REL, e.REL_WHEEL, int(a[1])); mouse.syn()
                elif a[0] == "sleep":
                    time.sleep(float(a[1]))

def send(cmds):
    with open(FIFO, "w") as f:
        for c in cmds:
            f.write(c + "\n")

if __name__ == "__main__":
    if sys.argv[1] == "daemon":
        daemon()
    else:
        send(sys.argv[2:])
