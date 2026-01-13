{
  description = "Sound Blaster X G7 Control for Linux - A native GUI application to control the Creative Sound Blaster X G6";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1"; # unstable Nixpkgs
    fenix = {
      url = "https://flakehub.com/f/nix-community/fenix/0.1";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, ... }@inputs:

    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forEachSupportedSystem =
        f:
        inputs.nixpkgs.lib.genAttrs supportedSystems (
          system:
          f {
            pkgs = import inputs.nixpkgs {
              inherit system;
              overlays = [
                inputs.self.overlays.default
              ];
            };
          }
        );
    in
    {
      overlays.default = final: prev: {
        rustToolchain =
          with inputs.fenix.packages.${prev.stdenv.hostPlatform.system};
          combine (
            with complete;
            [
              clippy
              rustc
              cargo
              rustfmt
              rust-src
            ]
          );

        blaster-x-g6-control = prev.callPackage ./nix/package.nix {
          rustPlatform = prev.makeRustPlatform {
            cargo = final.rustToolchain;
            rustc = final.rustToolchain;
          };
        };
      };

      packages = forEachSupportedSystem (
        { pkgs }:
        {
          default = pkgs.blaster-x-g6-control;
          blaster-x-g6-control = pkgs.blaster-x-g6-control;
          
          # Debian package output
          deb = pkgs.callPackage ./nix/deb.nix {
            rustPlatform = pkgs.makeRustPlatform {
              cargo = pkgs.rustToolchain;
              rustc = pkgs.rustToolchain;
            };
          };
        }
      );

      devShells = forEachSupportedSystem (
        { pkgs }:
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              udev 
              rustToolchain
              openssl
              pkg-config
              cargo-deny
              cargo-edit
              cargo-watch
              rust-analyzer
              bacon
              evcxr 

              # wayland support
              wayland
              libxkbcommon
              libGL
              libglvnd

              # python
              (pkgs.python312.withPackages (
                python-pkgs: with python-pkgs; [
                  libusb1 
                  hidapi 
                ]
              ))
            ];

            env = {
              # Required by rust-analyzer
              RUST_SRC_PATH = "${pkgs.rustToolchain}/lib/rustlib/src/rust/library";
              LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath [ pkgs.udev pkgs.wayland pkgs.libxkbcommon pkgs.libGL pkgs.libglvnd pkgs.openssl ]}";
            };
          };
        }
      );

      # NixOS module for easy installation
      nixosModules.default = { config, lib, pkgs, ... }:
        with lib;
        let
          cfg = config.hardware.soundblaster-g6;
        in
        {
          options.hardware.soundblaster-g6 = {
            enable = mkEnableOption "Sound Blaster X G6 support";
            commandName = mkOption {
              type = types.str;
              default = "linuxblaster";
              description = "The command name to make available in the system path.";
            };
          };

          config = mkIf cfg.enable {
            # Install the control application
            environment.systemPackages = [ 
              self.packages.${pkgs.system}.blaster-x-g6-control
              (pkgs.runCommand "linuxblaster-symlink" {} ''
                mkdir -p $out/bin
                ln -s ${self.packages.${pkgs.system}.blaster-x-g6-control}/bin/blaster_x_g6_control $out/bin/${cfg.commandName}
              '')
            ];

            # Add udev rules for device access
            services.udev.extraRules = ''
              # Creative Sound Blaster X G6
              SUBSYSTEM=="hidraw", ATTRS{idVendor}=="041e", ATTRS{idProduct}=="3256", MODE="0666"
            '';
          };
        };
    };
}
