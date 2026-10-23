{ lib
, rustPlatform
, pkg-config
, udev
, alsa-lib
, wayland
, libxkbcommon
, libGL
, libglvnd
, fetchFromGitHub
}:

rustPlatform.buildRustPackage rec {
  pname = "linuxblaster_control";
  version = "2.2.0";

  # only what the build needs, so target/ and other local files stay out of the store
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../build.rs
      ../src
      ../examples
      ../packaging
      ../README.md
      ../LICENSE
    ];
  };

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
  ];

  buildInputs = [
    udev
    alsa-lib
    wayland
    libxkbcommon
    libGL
    libglvnd
  ];

  # Runtime library path for dynamic linking
  runtimeDependencies = [
    udev
    alsa-lib
    wayland
    libxkbcommon
    libGL
    libglvnd
  ];

  # Skip tests that require HOME directory and device access
  doCheck = false;

  postInstall = ''
    # Install udev rules
    install -Dm644 ${../packaging/70-linuxblaster-control.rules} $out/lib/udev/rules.d/70-linuxblaster-control.rules

    # Application menu entry and icons
    install -Dm644 ${../packaging/linuxblaster-control.desktop} $out/share/applications/linuxblaster-control.desktop
    install -Dm644 ${../packaging/linuxblaster-control.png} $out/share/icons/hicolor/256x256/apps/linuxblaster-control.png
    install -Dm644 ${../packaging/linuxblaster-control.svg} $out/share/icons/hicolor/scalable/apps/linuxblaster-control.svg

    # README and LICENSE
    install -Dm644 ${../README.md} $out/share/doc/${pname}/README.md
    install -Dm644 ${../LICENSE} $out/share/licenses/${pname}/LICENSE
  '';

  postFixup = ''
    # Add runtime dependencies to the binary
    patchelf --add-rpath "${lib.makeLibraryPath [ udev alsa-lib wayland libxkbcommon libGL libglvnd ]}" $out/bin/linuxblaster_control
  '';

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

