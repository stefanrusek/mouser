#!/bin/bash
# InputCapture while a drag is in progress (GNOME). Needs gnome-headless.sh running.
. "$(dirname "$0")/env.sh"
pkill -f 'gtk_drag[.]py'; pkill spike_portals; sleep 1
($PROBE ic --timeout 100 --hold 60 --release-after > /tmp/ic3.out 2>&1 &)
sleep 5; $S/ic-approve.sh; sleep 1
grep -E "enable|barrier.*failed" /tmp/ic3.out
(python3 $S/gtk_drag.py 45 > /tmp/drag.out 2>&1 &)
sleep 4
$S/drive.py "moveto 960 540" "btndown 272" "wait 0.3" "move 40 0" "wait 0.3" "move 60 0" "wait 0.5"
echo "--- after drag start"; grep gtk_drag /tmp/drag.out
$S/drive.py "move 1500 0" "wait 1.5"
echo "--- after moving into the right edge with the button held"; grep -E "activ|gtk" /tmp/ic3.out /tmp/drag.out
$S/shot.py /tmp/d2.png
$S/drive.py "btnup 272" "wait 1.5"
echo "--- after button release"; grep -E "activ|gtk|eis.totals|release" /tmp/ic3.out /tmp/drag.out
