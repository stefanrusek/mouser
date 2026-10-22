# source me inside the VM
. ~/.cargo/env 2>/dev/null
if systemctl --user is-active -q mouser-kwin 2>/dev/null; then DESK=KDE; else DESK=GNOME; fi
export CARGO_TARGET_DIR=~/target-probes WAYLAND_DISPLAY=wayland-0 XDG_CURRENT_DESKTOP=$DESK XDG_SESSION_TYPE=wayland
export PROBE=~/target-probes/debug/spike_portals
export S=/Users/stefanrusek/Code/mouser/crates/mouser-platform-linux/probes/scripts
