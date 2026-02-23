{ lib
, rustPlatform
, pkg-config
, udev
, wayland
, libxkbcommon
, libGL
, libglvnd
, libusb1 
, hidapi
, makeDesktopItem
, copyDesktopItems
, fetchFromGitHub
}:

rustPlatform.buildRustPackage rec {
  pname = "linuxblaster_control";
  version = "2.0.0";

  src = lib.cleanSource ../.;

  autoeq_data = fetchFromGitHub {
    owner = "jaakkopasanen";
    repo = "AutoEq";
    rev = "master";
    sha256 = "sha256-/kj5ITqtaEWtcZkmB5DEIfj44otXKXYBE7ArV++YLkM=";
  };

  env.AUTOEQ_REPO_DIR = autoeq_data;

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
    libusb1 
    hidapi
  ];

  # Runtime library path for dynamic linking
  runtimeDependencies = [
    udev
    wayland
    libxkbcommon
    libGL
    libglvnd
    libusb1 
  ];

  # Skip tests that require HOME directory and device access
  doCheck = false;

  postInstall = ''
    # Install udev rules
    install -Dm644 ${./99-soundblaster-g6.rules} $out/lib/udev/rules.d/99-soundblaster-g6.rules

    # README and LICENSE
    install -Dm644 ${../README.md} $out/share/doc/${pname}/README.md
    install -Dm644 ${../LICENSE} $out/share/licenses/${pname}/LICENSE
    
    # Icon if it exists
    if [ -f ${../LinuxblasterControl.png} ]; then
      install -Dm644 ${../LinuxblasterControl.png} $out/share/pixmaps/linuxblaster_control.png
    fi
  '';

  postFixup = ''
    # Add runtime dependencies to the binary
    patchelf --add-rpath "${lib.makeLibraryPath [ udev wayland libxkbcommon libGL libglvnd libusb1 ]}" $out/bin/linuxblaster_control
  '';

  desktopItems = [
    (makeDesktopItem {
      name = "Linuxblaster Control";
      desktopName = "Linuxblaster Control";
      comment = "Control the Creative Sound Blaster X G6 USB DAC/Amp";
      exec = "linuxblaster_control";
      icon = "";
      categories = [ "Audio" "AudioVideo" "Settings" ];
      keywords = [ "sound" "audio" "equalizer" "dac" "creative" ];
      terminal = false;
    })
  ];

  meta = with lib; {
    description = "Native Linux GUI application to control the Creative Sound Blaster X G6";
    longDescription = ''
    '';
    homepage = "https://github.com/RizeCrime/linuxblaster_control";
    license = licenses.mit;
    maintainers = [ ];
    mainProgram = "linuxblaster_control";
    platforms = platforms.linux;
  };
}

