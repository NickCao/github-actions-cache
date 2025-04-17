{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ self.overlays.default ];
        };
      in
      rec {
        packages = {
          default = pkgs.github-actions-cache;
          github-actions-cache = pkgs.github-actions-cache;
        };
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rustfmt
            rust-analyzer
          ];
          inputsFrom = [ packages.default ];
        };
      }
    )
    // {
      overlays.default =
        final: _: with final; {
          github-actions-cache = rustPlatform.buildRustPackage rec {
            name = "github-actions-cache";
            src = lib.cleanSourceWith {
              src = self;
              filter =
                name: type:
                name == "${self}/build.rs"
                || lib.strings.hasPrefix "${self}/Cargo" name
                || lib.strings.hasPrefix "${self}/src" name
                || lib.strings.hasPrefix "${self}/github" name;
            };
            cargoLock = {
              lockFile = "${src}/Cargo.lock";
              outputHashes = {
                "twirp-0.7.0" = "sha256-IKOxlkWu8fP9F3RNgUezUEdQr2ADyWHQdKpMizlYMRY=";
              };
            };
            nativeBuildInputs = [ protobuf ];
          };
        };
    };
}
