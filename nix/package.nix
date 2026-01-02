{ lib
, rustPlatform
, pkg-config
, udev
, wayland
, libxkbcommon
, libGL
, libglvnd
, hidapi
, makeDesktopItem
, copyDesktopItems
}:

rustPlatform.buildRustPackage rec {
  pname = "blaster-x-g6-control";
  version = "1.1.0";

  src = lib.cleanSource ../.;

  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
    copyDesktopItems
  ];

  buildInputs = [
    udev
    wayland
    libxkbcommon
    libGL
    libglvnd
    hidapi
  ];

  # Runtime library path for dynamic linking
  runtimeDependencies = [
    udev
    wayland
    libxkbcommon
    libGL
    libglvnd
  ];

  # Skip tests that require HOME directory and device access
  doCheck = false;

  postInstall = ''
    # Install udev rules
    install -Dm644 ${./99-soundblaster-g6.rules} $out/lib/udev/rules.d/99-soundblaster-g6.rules
    
    # Install README and LICENSE
    install -Dm644 ${../README.md} $out/share/doc/${pname}/README.md
    install -Dm644 ${../LICENSE} $out/share/licenses/${pname}/LICENSE
    
    # Install icon if it exists
    if [ -f ${../LinuxblasterCommand.png} ]; then
      install -Dm644 ${../LinuxblasterCommand.png} $out/share/pixmaps/blaster-x-g6-control.png
    fi
  '';

  desktopItems = [
    (makeDesktopItem {
      name = "blaster-x-g6-control";
      desktopName = "Sound Blaster X G6 Control";
      comment = "Control Creative Sound Blaster X G6 audio settings";
      exec = "blaster_x_g6_control";
      icon = "blaster-x-g6-control";
      categories = [ "Audio" "AudioVideo" "Settings" ];
      keywords = [ "sound" "audio" "equalizer" "dac" "creative" ];
      terminal = false;
    })
  ];

  meta = with lib; {
    description = "Native Linux GUI application to control the Creative Sound Blaster X G6";
    longDescription = ''
      A native Linux GUI application to control the Creative Sound Blaster X G6 USB DAC/Amp.
      Built with Rust using egui for the interface and hidapi for USB HID communication.
      
      Features:
      - Surround Sound control with slider
      - Crystalizer audio enhancement
      - Bass boost
      - Smart Volume
      - Dialog Plus
      - Night Mode & Loud Mode
      - 10-Band Equalizer (31Hz – 16kHz, ±12 dB)
      - Preset Management for saving and loading configurations
    '';
    homepage = "https://github.com/RizeCrime/linuxblaster_control";
    license = licenses.mit;
    maintainers = [ ];
    mainProgram = "blaster_x_g6_control";
    platforms = platforms.linux;
  };
}

