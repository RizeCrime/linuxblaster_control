{
  description = "Temporary Wireshark setup for USB sniffing";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        
        # We create a shell script that handles the setup logic
        snifferScript = pkgs.writeShellScriptBin "sniff-usb" ''
          echo "🛠️  Setting up USB monitoring environment..."
          
          # 1. Load the kernel module (requires sudo)
          if ! lsmod | grep -q usbmon; then
            echo "   Loading 'usbmon' kernel module..."
            sudo modprobe usbmon
          fi

          # 2. Grant permission to the current user (ACL) so we don't run Wireshark as root
          #    This gives your user read-access to the USB monitors temporarily.
          echo "   Setting permissions on /dev/usbmon*..."
          sudo setfacl -m u:$USER:r /dev/usbmon*

          # 3. Launch Wireshark
          echo "🚀 Launching Wireshark..."
          nohup ${pkgs.wireshark}/bin/wireshark >/dev/null 2>&1 &
        '';

      in {
        # This allows you to run `nix run`
        apps.default = {
          type = "app";
          program = "${snifferScript}/bin/sniff-usb";
        };

        # This allows you to enter `nix develop` if you prefer a shell
        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.wireshark
            pkgs.acl      # For setfacl
            snifferScript
            pkgs.usbutils
          ];
          
          shellHook = ''
            echo "Environment ready!"
            echo "Run 'sniff-usb' to set up permissions and start Wireshark."
          '';
        };
      }
    );
}
