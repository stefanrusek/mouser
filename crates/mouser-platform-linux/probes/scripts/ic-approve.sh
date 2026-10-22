#!/bin/bash
# Approve the GNOME 46 "Capture Input" dialog (toggle + Share) using synthetic pointer input.
. "$(dirname "$0")/env.sh"
$S/drive.py "moveto 1140 530" "click 272" "moveto 1167 428" "click 272" "wait 1.5"
