{
  description = "Build a cargo project without extra checks";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      crane,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        inherit (pkgs.darwin.apple_sdk_11_0.frameworks) Accelerate CoreAudio MetalKit;
        inherit (pkgs) lib stdenv;

        craneLib = crane.mkLib pkgs;
        darwinBuildInputs = lib.optionals stdenv.isDarwin [
          Accelerate
          CoreAudio
          MetalKit
        ];

        # Common arguments can be set here to avoid repeating them later
        # Note: changes here will rebuild all dependency crates
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          doCheck = false;

          nativeBuildInputs = [ pkgs.cmake ];
          buildInputs = [
            pkgs.iconv
            pkgs.rustPlatform.bindgenHook
          ] ++ darwinBuildInputs;
        };

        buildTarget =
          name:
          craneLib.buildPackage (
            commonArgs
            // {
              pname = "voice-${name}";
              cargoExtraArgs = "-p ${name}";
              cargoArtifacts = craneLib.buildDepsOnly commonArgs;
            }
          );
      in
      {
        packages = {
          default = (buildTarget "client");
          client = (buildTarget "client");
          llm = (buildTarget "llm");
          server = (buildTarget "server");
        };
        devShells.default = craneLib.devShell {
          # Inherit inputs from checks.
          packages = (with commonArgs; nativeBuildInputs ++ buildInputs) ++ [
            pkgs.rust-analyzer
          ];
        };
      }
    );
}
