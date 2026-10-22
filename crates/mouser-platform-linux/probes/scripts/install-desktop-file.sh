#!/bin/bash
# The app ID `com.mouser.app` needs a matching .desktop file for xdg-desktop-portal's host-app registry.
mkdir -p ~/.local/share/applications
cat > ~/.local/share/applications/com.mouser.app.desktop <<'D'
[Desktop Entry]
Type=Application
Name=Mouser
Exec=/bin/true
Terminal=false
NoDisplay=true
D
update-desktop-database ~/.local/share/applications 2>/dev/null || true
