#!/bin/bash
# Provision a Lima Ubuntu VM (24.04 / 26.04) for the platform-linux probes and gate. Idempotent.
set -e
sudo DEBIAN_FRONTEND=noninteractive apt-get update -qq
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq build-essential pkg-config curl git xvfb dbus-x11 xdotool x11-utils x11-apps \
  gnome-shell xdg-desktop-portal xdg-desktop-portal-gnome mutter libei-dev libei1 libeis1 libxcb1-dev libxkbcommon-dev libdbus-1-dev \
  libx11-dev libxi-dev libxtst-dev libxfixes-dev libxrandr-dev gnome-keyring libsecret-tools python3-gi python3-dbus gir1.2-glib-2.0 \
  gir1.2-gtk-3.0 gir1.2-gtk-4.0 evtest libglib2.0-dev clang gnome-session at-spi2-core nodejs pipewire gstreamer1.0-pipewire \
  gstreamer1.0-plugins-good gstreamer1.0-plugins-base gstreamer1.0-tools wireplumber xserver-xorg-core xserver-xorg-video-dummy \
  xserver-xorg-input-libinput python3-evdev libinput-tools
command -v rustup >/dev/null || curl -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none
. ~/.cargo/env
loginctl enable-linger "$USER"
