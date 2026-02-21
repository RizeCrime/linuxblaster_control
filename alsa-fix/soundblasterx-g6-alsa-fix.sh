#!/bin/sh
# soundblasterx-g6-alsa-fix.sh
# Fixes ALSA mixer defaults for the SoundBlasterX G6 on Linux.
# Triggered automatically by udev when the card is detected.
#
# The snd-usb-audio driver doesn't set the correct capture source,
# so the microphone doesn't work out of the box. This script fixes that
# and mutes unused inputs to prevent feedback/bleed.

CARD_NAME="Sound BlasterX G6"
LOG_TAG="g6-alsa-fix"
MAX_RETRIES=10
RETRY_DELAY=1

log() {
    if command -v logger > /dev/null 2>&1; then
        logger -t "$LOG_TAG" "$1"
    fi
}

# Find the card index. Retry because ALSA enumeration may still be settling.
card_index=""
attempt=0
while [ "$attempt" -lt "$MAX_RETRIES" ]; do
    card_index=$(aplay -l 2>/dev/null | grep "$CARD_NAME" | head -n1 | sed 's/^card \([0-9]*\).*/\1/')
    if [ -n "$card_index" ]; then
        break
    fi
    attempt=$((attempt + 1))
    sleep "$RETRY_DELAY"
done

if [ -z "$card_index" ]; then
    log "ERROR: Could not find '$CARD_NAME' after $MAX_RETRIES attempts"
    exit 1
fi

log "Found '$CARD_NAME' as card $card_index, applying fixes..."

# === THE CRITICAL FIX ===
# Without this, the microphone simply doesn't work on Linux.
amixer -c "$card_index" -q cset name='PCM Capture Source' 'External Mic' 2>/dev/null

# === Mute unused playback inputs (prevents hearing yourself / feedback) ===
for input in 'External Mic' 'Line In' 'S/PDIF In'; do
    amixer -c "$card_index" -q cset "name=${input} Playback Volume" 0 2>/dev/null
    amixer -c "$card_index" -q cset "name=${input} Playback Switch" off 2>/dev/null
done

# === Mute unused capture inputs (only External Mic should be active) ===
for input in 'Line In' 'S/PDIF In' 'What U Hear'; do
    amixer -c "$card_index" -q cset "name=${input} Capture Volume" 0 2>/dev/null
    amixer -c "$card_index" -q cset "name=${input} Capture Switch" off 2>/dev/null
done

log "Successfully applied ALSA fixes for card $card_index"
