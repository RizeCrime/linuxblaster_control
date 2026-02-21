# SoundBlasterX G6 — ALSA Fix

Automatic fix for the SoundBlasterX G6 microphone not working on Linux.

The `snd-usb-audio` kernel driver doesn't set the correct default capture
source for the G6. This means the microphone doesn't work out of the box on
any Linux distro. Users have to manually run `amixer` commands every time.

## The Fix

A udev rule detects when the G6's ALSA sound card appears and automatically
runs a small script that:

1. Sets `PCM Capture Source` to `External Mic` (the critical fix)
2. Mutes unused inputs to prevent feedback/bleed

**Works on any distro.** Only requires `alsa-utils` (specifically `amixer` and
`aplay`), which is installed on essentially every Linux system with sound.

## Install

Install by running the `install.sh` script. 
That's it. Plug in your G6 and it just works (hopefully).

## Uninstall

```sh
sudo ./install.sh --uninstall
```

## NixOS Users

A Nix flake is provided. Don't use the install script — use the NixOS module
instead.

**1. Add the flake input** (adjust the URL to wherever you're hosting this):

```nix
# flake.nix
inputs.g6-fix.url = "github:RizeCrime/linuxblaster_command?dir=alsa-fix";
```

**2. Enable the module** in your NixOS configuration:

```nix
# configuration.nix
{ inputs, ... }: {
  imports = [ inputs.g6-fix.nixosModules.default ];

  hardware.soundblasterx-g6.enable = true;

  # Mutes Line In, S/PDIF, and What U Hear to prevent feedback.
  # Set to false if you actually use those inputs.
  # hardware.soundblasterx-g6.muteUnusedInputs = false;
}
```

**3. Rebuild:**

```sh
sudo nixos-rebuild switch
```

## How It Works

```
G6 plugged in
  → kernel loads snd-usb-audio
    → ALSA registers sound card
      → udev sees SUBSYSTEM=="sound" event with G6's USB IDs
        → runs fix script
          → amixer sets correct capture source + mutes unused inputs
```

No services. No daemons. No polling. Just a one-shot script triggered by
hardware detection.
