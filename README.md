# Sound Blaster X G6 Control for Linux

A native Linux GUI application to control the **Creative Sound Blaster X G6** USB DAC/Amp. Built with Rust using egui for the interface and hidapi for USB HID communication.

![Screenshot](LinuxblasterCommand.png)

## Supported Features

Toggles and sliders work for:

- Surround Sound
- Crystalizer
- Bass
- Smart Volume
- Dialog Plus
- Night Mode & Loud Mode 
- 10-Band Equalizer (31Hz – 16kHz, ±12 dB)

**Preset Management** — Save and load custom configurations

## Presets

Presets are stored as JSON files in `~/.config/blaster_x_g6_control/presets/`.

> [!IMPORTANT]
> The preset format is custom to this application and is **not compatible** with official Creative Sound Blaster Command profiles (.json or .xml) from Windows.

## Requirements

- Linux (tested on x86_64)
- Sound Blaster X G6 connected via USB
- udev rules for HID access (see below)

### udev Rules

To access the device without root privileges, create a udev rule:

```bash
sudo tee /etc/udev/rules.d/99-soundblaster-g6.rules << 'EOF'
# Creative Sound Blaster X G6
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="041e", ATTRS{idProduct}=="3256", MODE="0666"
EOF

sudo udevadm control --reload-rules
sudo udevadm trigger
```

You may need to unplug and replug the device after adding the rule.

## Installation

### Option 1: Download Pre-built Package (Recommended)

**Debian/Ubuntu (.deb package)**

Download the latest `.deb` file from the [GitHub Releases](https://github.com/RizeCrime/linuxblaster_control/releases) page and install:

```bash
sudo dpkg -i blaster-x-g6-control_1.1.0_amd64.deb
sudo apt-get install -f  # Install any missing dependencies
```

This will install the application, desktop file, icon, and udev rules automatically.

### Option 2: Nix Package Manager

**NixOS (with flakes)**

Add to your NixOS configuration:

```nix
{
  inputs.blaster-x-g6-control.url = "github:RizeCrime/linuxblaster_control";

  outputs = { self, nixpkgs, blaster-x-g6-control, ... }: {
    nixosConfigurations.yourhostname = nixpkgs.lib.nixosSystem {
      modules = [
        blaster-x-g6-control.nixosModules.default
        {
          hardware.soundblaster-g6.enable = true;
        }
      ];
    };
  };
}
```

**Any Linux with Nix (with flakes)**

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

### Option 3: Building from Source

Requires Rust 2024 edition (nightly) and system dependencies for hidapi and egui.

```bash
# Clone the repository
git clone https://github.com/RizeCrime/linuxblaster_control.git
cd linuxblaster_control

# Build release binary
cargo build --release

# Run
./target/release/blaster_x_g6_control
```

### System Dependencies

On Debian/Ubuntu:

```bash
sudo apt install libudev-dev libhidapi-dev libwayland-dev libxkbcommon-dev libgl1-mesa-dev
```

On Fedora:

```bash
sudo dnf install systemd-devel hidapi-devel wayland-devel libxkbcommon-devel mesa-libGL-devel
```

On Arch:

```bash
sudo pacman -S hidapi wayland libxkbcommon mesa
```

### Nix Development Shell

A `flake.nix` is provided for Nix users who want to develop:

```bash
nix develop  # Enter development shell with all dependencies
cargo build --release
```

## Usage

Simply run the application while the Sound Blaster X G6 is connected:

```bash
./blaster_x_g6_control
```

If the device is not detected, the UI will display a warning but remain functional for previewing the interface.

## ⚠️ Development Status

**This project is in active development and should be considered experimental.**

### Current Limitations

- **No state reading** — Cannot read current device state from the hardware on startup; the application starts with default values.
- **Limited testing** — Tested only on the developer's hardware.

### Known Issues

- **Night Mode and Loud Mode are currently broken.** These features do not seem to work as expected with the current reverse-engineered protocol.
- The USB protocol was reverse-engineered and may not cover all device features.
- Some features available in the Windows Sound Blaster Command software are not yet implemented.

## Technical Details

- **Vendor ID:** `0x041e` (Creative Technology)
- **Product ID:** `0x3256` (Sound Blaster X G6)
- **Interface:** 4 (HID control interface)

Communication uses 65-byte HID reports with a custom protocol consisting of DATA and COMMIT packets.

## Contributing

Contributions are welcome! If you have a Sound Blaster X G6 and want to help:

- Report bugs or missing features
- Help reverse-engineer additional functionality
- Improve the UI/UX
- Add config file support

## Acknowledgments

This project builds upon the USB protocol research from the [soundblaster-x-g6-cli](https://github.com/nils-skowasch/soundblaster-x-g6-cli) project by Nils Skowasch, which provided initial USB packet captures and protocol documentation. Additional reverse engineering (including the 10-band EQ protocol) was done for this project.

## AI Disclaimer 

As much as I enjoyed reverse engineering the protocol and writing the backend, I only have surface-level knowledge of egui/eframe and don't much care for GUI design.
As such, about half of the [UI](src/ui.rs) was written by AI.
This README was also beautified by AI.

## License

MIT License — see [LICENSE](LICENSE) for details.
