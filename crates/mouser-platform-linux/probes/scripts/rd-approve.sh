#!/bin/bash
# Approve the GNOME 46 "Remote Desktop" dialog (toggle remote interaction [+ clipboard already on] + Share).
. "$(dirname "$0")/env.sh"
$S/drive.py "moveto 1205 463" "click 272" "moveto 1247 328" "click 272" "wait 1.5"
