#!/bin/bash
# KWin only serves ScreenShot2 to executables named by a desktop file that lists it as a restricted interface.
PY=$(readlink -f "$(command -v python3)")
mkdir -p ~/.local/share/applications
cat > ~/.local/share/applications/mouser-probe-shot.desktop <<D
[Desktop Entry]
Type=Application
Name=Mouser probe screenshot
Exec=$PY %f
NoDisplay=true
X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
D
