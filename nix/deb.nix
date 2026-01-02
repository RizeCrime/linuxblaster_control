{ stdenv
, lib
, rustPlatform
, pkg-config
, udev
, wayland
, libxkbcommon
, libGL
, libglvnd
, hidapi
, makeDesktopItem
, dpkg
, fakeroot
}:

let
  # Build a portable version for .deb distribution
  portablePackage = rustPlatform.buildRustPackage rec {
    pname = "blaster-x-g6-control";
    version = "1.1.0";

    src = lib.cleanSource ../.;

    cargoLock = {
      lockFile = ../Cargo.lock;
    };

    nativeBuildInputs = [ pkg-config ];

    buildInputs = [
      udev
      wayland
      libxkbcommon
      libGL
      libglvnd
      hidapi
    ];

    # Skip tests for the portable build
    doCheck = false;
  };

  desktopFile = makeDesktopItem {
    name = "blaster-x-g6-control";
    desktopName = "Sound Blaster X G6 Control";
    comment = "Control Creative Sound Blaster X G6 audio settings";
    exec = "blaster_x_g6_control";
    icon = "blaster-x-g6-control";
    categories = [ "Audio" "AudioVideo" "Settings" ];
    keywords = [ "sound" "audio" "equalizer" "dac" "creative" ];
    terminal = false;
  };

in
stdenv.mkDerivation rec {
  pname = "blaster-x-g6-control-deb";
  version = portablePackage.version;

  src = portablePackage;

  nativeBuildInputs = [ dpkg fakeroot ];

  dontUnpack = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall

    # Create debian package structure in a temporary directory
    DEBDIR=$(mktemp -d)
    mkdir -p $DEBDIR/DEBIAN
    mkdir -p $DEBDIR/usr/bin
    mkdir -p $DEBDIR/usr/share/applications
    mkdir -p $DEBDIR/usr/share/pixmaps
    mkdir -p $DEBDIR/usr/share/doc/${portablePackage.pname}
    mkdir -p $DEBDIR/lib/udev/rules.d

    # Copy binary
    cp ${portablePackage}/bin/blaster_x_g6_control $DEBDIR/usr/bin/
    chmod 755 $DEBDIR/usr/bin/blaster_x_g6_control

    # Copy desktop file
    cp ${desktopFile}/share/applications/*.desktop $DEBDIR/usr/share/applications/

    # Copy icon if available
    if [ -f ${../LinuxblasterCommand.png} ]; then
      cp ${../LinuxblasterCommand.png} $DEBDIR/usr/share/pixmaps/blaster-x-g6-control.png
    fi

    # Copy documentation with proper names
    if [ -f ${../README.md} ]; then
      cp ${../README.md} $DEBDIR/usr/share/doc/${portablePackage.pname}/README.md
    fi
    if [ -f ${../LICENSE} ]; then
      cp ${../LICENSE} $DEBDIR/usr/share/doc/${portablePackage.pname}/LICENSE
    fi

    # Copy udev rules
    cp ${./99-soundblaster-g6.rules} $DEBDIR/lib/udev/rules.d/99-soundblaster-g6.rules

    # Create control file
    cat > $DEBDIR/DEBIAN/control << EOF
Package: ${portablePackage.pname}
Version: ${version}
Section: sound
Priority: optional
Architecture: amd64
Depends: libudev1, libwayland-client0, libxkbcommon0, libgl1, libhidapi-hidraw0
Maintainer: RizeCrime <https://github.com/RizeCrime>
Description: Sound Blaster X G6 Control for Linux
 A native Linux GUI application to control the Creative Sound Blaster X G6
 USB DAC/Amp. Built with Rust using egui for the interface and hidapi for
 USB HID communication.
 .
 Features:
  - Surround Sound control with slider
  - Crystalizer audio enhancement
  - Bass boost
  - Smart Volume
  - Dialog Plus
  - Night Mode & Loud Mode
  - 10-Band Equalizer (31Hz – 16kHz, ±12 dB)
  - Preset Management for saving and loading configurations
Homepage: https://github.com/RizeCrime/linuxblaster_control
EOF

    # Create postinst script to reload udev rules (using /bin/sh for portability)
    cat > $DEBDIR/DEBIAN/postinst << 'EOF'
#!/bin/sh
set -e

if [ "$1" = "configure" ]; then
    # Reload udev rules
    if command -v udevadm > /dev/null 2>&1; then
        udevadm control --reload-rules || true
        udevadm trigger || true
    fi
fi

exit 0
EOF
    chmod 755 $DEBDIR/DEBIAN/postinst

    # Create postrm script to reload udev rules on removal
    cat > $DEBDIR/DEBIAN/postrm << 'EOF'
#!/bin/sh
set -e

if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
    # Reload udev rules
    if command -v udevadm > /dev/null 2>&1; then
        udevadm control --reload-rules || true
        udevadm trigger || true
    fi
fi

exit 0
EOF
    chmod 755 $DEBDIR/DEBIAN/postrm

    # Build the .deb file
    mkdir -p $out
    dpkg-deb --build $DEBDIR $out/${portablePackage.pname}_${version}_amd64.deb

    # Clean up temp directory
    rm -rf $DEBDIR

    runHook postInstall
  '';

  meta = with lib; {
    description = "Debian package for ${portablePackage.meta.description or "Sound Blaster X G6 Control"}";
    homepage = "https://github.com/RizeCrime/linuxblaster_control";
    license = licenses.mit;
    platforms = platforms.linux;
  };
}

