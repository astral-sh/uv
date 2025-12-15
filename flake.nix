{
  description = "uv – fast Python package manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages.default = pkgs.stdenv.mkDerivation {
          name = "uv";
          buildCommand = ''
            mkdir -p $out/bin
            curl -LsSf https://astral.sh/uv/install.sh | sh
            cp -r ~/.local/bin/uv $out/bin/
          '';
        };

        apps.uv = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };

        apps.default = self.apps.${system}.uv;
      });
}
