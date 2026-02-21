#!/bin/sh
# install.sh — One-time setup for SoundBlasterX G6 ALSA fixes
# Run with: sudo ./install.sh
# Uninstall with: sudo ./install.sh --uninstall
#
# What this does:
#   1. Installs a small script that fixes ALSA mixer defaults for the G6
#   2. Installs a udev rule that runs the script automatically when the G6 is detected
#   That's it. No services, no daemons, no cron jobs.

set -e

SCRIPT_SRC="soundblasterx-g6-alsa-fix.sh"
RULES_SRC="91-soundblasterx-g6.rules"
SCRIPT_DEST="/usr/local/bin/soundblasterx-g6-alsa-fix.sh"
RULES_DEST="/etc/udev/rules.d/91-soundblasterx-g6.rules"

# --- Helpers ---

die() { echo "ERROR: $1" >&2; exit 1; }

check_root() {
    if [ "$(id -u)" -ne 0 ]; then
        die "This script must be run as root (sudo ./install.sh)"
    fi
}

check_deps() {
    for cmd in amixer aplay udevadm; do
        if ! command -v "$cmd" > /dev/null 2>&1; then
            die "Required command '$cmd' not found. Please install alsa-utils."
        fi
    done
}

# --- Install ---

install() {
    check_deps

    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

    [ -f "$SCRIPT_DIR/$SCRIPT_SRC" ] || die "Cannot find $SCRIPT_SRC in $SCRIPT_DIR"
    [ -f "$SCRIPT_DIR/$RULES_SRC" ] || die "Cannot find $RULES_SRC in $SCRIPT_DIR"

    echo "Installing SoundBlasterX G6 ALSA fix..."

    # Install the fix script
    install -m 755 "$SCRIPT_DIR/$SCRIPT_SRC" "$SCRIPT_DEST"
    echo "  ✓ Installed $SCRIPT_DEST"

    # Install the udev rule
    install -m 644 "$SCRIPT_DIR/$RULES_SRC" "$RULES_DEST"
    echo "  ✓ Installed $RULES_DEST"

    # Reload udev rules
    udevadm control --reload-rules
    echo "  ✓ Reloaded udev rules"

    echo ""
    echo "Done! The fix will apply automatically whenever the G6 is plugged in."
    echo ""

    # If the G6 is currently connected, offer to apply the fix now
    if aplay -l 2>/dev/null | grep -q "Sound BlasterX G6"; then
        echo "G6 detected — applying fix now..."
        "$SCRIPT_DEST"
        echo "  ✓ Fix applied to current session"
    else
        echo "Plug in your G6 and it'll be configured automatically."
    fi
}

# --- Uninstall ---

uninstall() {
    echo "Uninstalling SoundBlasterX G6 ALSA fix..."

    [ -f "$SCRIPT_DEST" ] && rm -f "$SCRIPT_DEST" && echo "  ✓ Removed $SCRIPT_DEST"
    [ -f "$RULES_DEST" ] && rm -f "$RULES_DEST" && echo "  ✓ Removed $RULES_DEST"

    udevadm control --reload-rules 2>/dev/null
    echo "  ✓ Reloaded udev rules"
    echo ""
    echo "Done! ALSA fix has been removed."
}

# --- Main ---

check_root

case "${1:-}" in
    --uninstall|-u)
        uninstall
        ;;
    --help|-h)
        echo "Usage: sudo ./install.sh [--uninstall]"
        echo ""
        echo "Installs a udev rule + script that automatically fixes ALSA"
        echo "mixer defaults for the SoundBlasterX G6 whenever it's plugged in."
        ;;
    *)
        install
        ;;
esac
