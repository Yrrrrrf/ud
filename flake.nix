{
  description = "ud — universal dependency updater development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
            pkg-config
            openssl
          ];

          shellHook = ''
            echo "Welcome to the ud development shell!"
            echo "rustc: $(rustc --version)"
            echo "cargo: $(cargo --version)"
          '';
        };
      }
    );
}
