# G6 cold-boot helper (optional)

At cold boot the G6 has two firmware quirks that the app cannot fix from
userspace while it is running:

1. Its HID endpoint does not answer until the device receives a USB reset.
2. Its first USB enumeration reports an illegal UAC clock source
   (`selector reported illegal value, id 17`), so `snd-usb-audio` builds a
   card with a broken clock tree and playback stays silent.

A `USBDEVFS_RESET` wakes the HID endpoint, but it also leaves the audio
interface unusable until the next full re-enumeration — which is why the app
never resets the device itself. The fix has to happen once, at boot, before
anything opens the audio device. That is what this helper does.

## What it installs

| Path | Purpose |
|---|---|
| `/etc/udev/rules.d/71-g6-coldboot.rules` | starts the unit from the G6's udev `add` event |
| `/etc/systemd/system/g6-coldboot-prep.service` | oneshot, udev-triggered, not in the boot transaction |
| `/usr/local/libexec/g6-coldboot-prep.sh` | reset + authorized toggle + HID check + `modprobe` |
| `/etc/modprobe.d/99-g6-coldboot.conf` | holds `snd-usb-audio` back from the first enumeration |

The unit is not ordered before the display manager and no target pulls it in,
so `systemd-analyze critical-chain graphical.target` does not contain it and
boot is not delayed.

## Install

```sh
sudo ./install.sh
```

The System pane of `linuxblaster_control` has the same thing behind a button,
with a graphical password prompt (`pkexec`).

Requires `bash`, `python3` (for the hidraw ping and the reset ioctl), `udev`,
`systemd` and `logger`. Without `python3` the helper still installs and logs,
but skips the reset — which is the part that wakes the HID endpoint.

## Side effects

- **`snd-usb-audio` is blacklisted for module auto-loading.** The helper loads
  it explicitly after the G6 has been reset and re-enumerated. The blacklist is
  per module, not per device, so while it is installed and no G6 has been
  prepared in the current boot, any *other* USB audio device also needs
  `sudo modprobe snd-usb-audio` once. Replugging the G6 is not required.
- **One USB reset per boot.** A later replug does not reset again
  (`/run/g6-coldboot-done` gates it).
- **The device briefly disappears** (roughly two seconds) while it
  re-enumerates. PipeWire re-links the downmix filter chain by node name, so
  an active virtual-sink graph recovers on its own.
- Nothing is written to the G6's NVRAM and no audio is played.

## Verify after a reboot

```sh
journalctl -t g6-coldboot -b --no-pager
```

A healthy cold boot ends with:

```
found G6 at /sys/bus/usb/devices/3-1; waiting for firmware
initial HID ping: fail
USBDEVFS_RESET /dev/bus/usb/003/003
G6 re-enumerated after reset (devnum 3 -> 4)
loaded snd-usb-audio after clean re-enumeration
HID ping ok
G6 cold-boot preparation complete (hid_ok=1)
```

Then check that the boot was not delayed and that the kernel is clean:

```sh
systemd-analyze critical-chain graphical.target | grep -c g6-coldboot   # 0
journalctl -k -b | grep -c __uac_clock_find_source                      # 0
```

## Remove

```sh
sudo ./install.sh --remove
```

Then `sudo modprobe snd-usb-audio` if the module is still held back in the
current boot.

## Why this is not installed automatically

It needs root, it blacklists a kernel module at boot, and it resets a USB
device. Those are not things a desktop app should do silently, so the install
is an explicit, single action with a password prompt and a written list of
side effects.
