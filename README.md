# Sound Blaster X G6 Control for Linux

A native Linux GUI application to control the **Creative Sound BlasterX G6** USB DAC/Amp, with full [AutoEq](https://github.com/jaakkopasanen/AutoEq) integration. 

![Screenshot](LinuxblasterControl.png)

## Supported Features

- **Profiles**
  - Saving & Loading
- **SBX**
  - Surround Sound
  - Dialog+
  - Smart Volume 
  - Crystalizer 
  - Bass 
  - Equalizer 
    - PreAmp
    - 10-band EQ
- **Scout Mode** 
- Reset All(-ish)

## Not-Yet-Implemented Features

- **Profiles** 
  - Profile Save Location
- **SBX** 
  - Smart Volume Sub-Features
    - Night Mode & Loud Mode
  - Equalizer Sub-Features 
    - SBC Eq Presets 
- **Playback**
  - Direct Mode 
  - Output Select
  - Output Toggle 
  - Filter
  - Audio Quality 
- **Recording**
  - _Everything_
- **Decoder** 
  - "Normal", "Full", and "Night" Selection
- **Mixer** 
  - Output
    - Speakers 
  - Monitoring 
    - _Everything_ 
  - Recording
    - _Everything_ 
- **Lighting**
  - _Everything_

## Presets

Presets are stored as JSON files in `~/.local/share/linuxblaster/presets/`.

> [!IMPORTANT]
> The preset format is custom to this application and is **not compatible** with official Creative Sound Blaster Command profiles (.json or .xml) from Windows.

## Requirements

- Linux (tested on x86_64)
- Sound Blaster X G6 connected via USB
- udev rules for HID access (see below)

### udev Rules

To access the device without root privileges, create a udev rule:

``` js (not actually js, just makes for a good highlighting)
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="041e", ATTRS{idProduct}=="3256", MODE="0666"
```

You may need to unplug and replug the device after adding the rule.

## Installation

### Nix

Add to your NixOS configuration:

```nix
{
  inputs.linuxblaster_control.url = "github:RizeCrime/linuxblaster_control";

  outputs = { self, nixpkgs, linuxblaster_control, ... }: {
    nixosConfigurations.yourhostname = nixpkgs.lib.nixosSystem {
      modules = [
        linuxblaster_control.nixosModules.default
        {
          hardware.soundblaster-g6.enable = true;
        }
      ];
    };
  };
}
```

or 
```bash
# Run without installing
nix run github:RizeCrime/linuxblaster_control

# Install to user profile
nix profile install github:RizeCrime/linuxblaster_control

# Build locally
git clone https://github.com/RizeCrime/linuxblaster_control.git
cd linuxblaster_control
nix build
```

### Building from Source

```bash
# Clone the repository
git clone https://github.com/RizeCrime/linuxblaster_control.git
cd linuxblaster_control

# Build release binary
cargo build --release

# Run
./target/release/linuxblaster_control
```

### System Dependencies

You will need the following packages at a minimum:
- hidapi 
- udev

### Nix Development Shell

A `flake.nix` is provided for Nix users who want to develop:

```bash
nix develop  # Enter development shell with all dependencies
cargo build --release
```

## Usage

Simply run the application while the Sound Blaster X G6 is connected:

```bash
./linuxblaster_command
```

If the device is not detected, the application won't start. 
In that case, Launch it from a cli and check the logs (if I configured them correctly, which I'm not too sure about). 

## ⚠️ Development Status

**This project is in active development and should be considered experimental.**

### Current Limitations

- **No state reading** — Cannot read current device state from the hardware on startup; the application starts with default values.
- **Limited testing** — Tested only on the developer's hardware.
- The USB protocol was reverse-engineered and may not cover all device features.
- Some features available in the Windows Sound Blaster Command software are not yet implemented.

## Technical Details

- **Vendor ID:** `0x041e` (Creative Technology)
- **Product ID:** `0x3256` (Sound Blaster X G6)
- **Interface:** 4 (HID control interface)

Communication uses 65-byte HID reports with a custom protocol consisting of DATA and COMMIT packets.

Find the details in [UsbProtocol](UsbProtocol.md) (and [usb-spec](usb-spec.txt)).

## Contributing

Contributions are welcome! If you have a Sound Blaster X G6 and want to help:

- Report bugs or missing features
- Help reverse-engineer additional functionality

## Acknowledgments

This project builds upon the USB protocol research from the [soundblaster-x-g6-cli](https://github.com/nils-skowasch/soundblaster-x-g6-cli) project by Nils Skowasch, which provided initial USB packet captures and protocol documentation. Additional reverse engineering (including the 10-band EQ protocol) was done for this project.

## AI Disclaimer 

With v2 I can proudly say that all Code was written by me (In v1 the GUI was written by AI (and you could tell)). 
I do, however, use AI liberally to beautify anything user-facing (including this README and most other .md files); I have spent my entire holidays on this project in ADHD Hyperfocus mode, you do **not** want to see my raw documentation... might be enough for a diagnosis on its own. 

## License

MIT License — see [LICENSE](LICENSE) for details.
