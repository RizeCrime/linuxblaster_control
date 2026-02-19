{
  description = "SBX G6 USB debug environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      python = pkgs.python3.withPackages (ps: [ ps.pyusb ]);
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [ python pkgs.libusb1 ];

        shellHook = ''
          echo "SBX G6 USB debug shell"
          echo "pyusb ready — run: python3 test_usb.py"
        '';
      };
    };
}
