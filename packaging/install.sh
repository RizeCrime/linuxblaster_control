#!/bin/sh
# Installs Linuxblaster Control from the downloaded archive.
#
#   sudo ./install.sh              install
#   sudo ./install.sh --uninstall  remove everything again
#
# Installs to /usr/local, plus the udev rule that lets your user
# account control the G6 without root.

set -e

PREFIX=/usr/local
RULE=/etc/udev/rules.d/70-linuxblaster-control.rules
HERE=$(cd "$(dirname "$0")" && pwd)

if [ "$(id -u)" -ne 0 ]; then
    echo "Please run this with sudo:  sudo $0 $*"
    exit 1
fi

reload_udev() {
    if command -v udevadm >/dev/null 2>&1; then
        udevadm control --reload-rules >/dev/null 2>&1 || true
        udevadm trigger --subsystem-match=hidraw --action=change >/dev/null 2>&1 || true
    fi
}

if [ "$1" = "--uninstall" ]; then
    rm -f "$PREFIX/bin/linuxblaster_control" \
          "$PREFIX/share/applications/linuxblaster-control.desktop" \
          "$PREFIX/share/icons/hicolor/256x256/apps/linuxblaster-control.png" \
          "$PREFIX/share/icons/hicolor/scalable/apps/linuxblaster-control.svg" \
          "$RULE"
    reload_udev
    echo "Linuxblaster Control was removed."
    echo "Your saved profiles in ~/.local/share/linuxblaster/ were kept."
    exit 0
fi

install -Dm755 "$HERE/linuxblaster_control" "$PREFIX/bin/linuxblaster_control"
install -Dm644 "$HERE/linuxblaster-control.desktop" "$PREFIX/share/applications/linuxblaster-control.desktop"
install -Dm644 "$HERE/linuxblaster-control.png" "$PREFIX/share/icons/hicolor/256x256/apps/linuxblaster-control.png"
install -Dm644 "$HERE/linuxblaster-control.svg" "$PREFIX/share/icons/hicolor/scalable/apps/linuxblaster-control.svg"
install -Dm644 "$HERE/70-linuxblaster-control.rules" "$RULE"
reload_udev

# refresh the application menu, where available
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$PREFIX/share/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" || true

echo "Linuxblaster Control was installed."
echo "Start it from your application menu, or run: linuxblaster_control"
