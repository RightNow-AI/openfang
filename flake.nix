{
  description = "The OpenFang Agent OS";
  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-flake.url = "github:juspay/rust-flake";
  };
  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [
        inputs.rust-flake.flakeModules.default
        inputs.rust-flake.flakeModules.nixpkgs
      ];
      systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin"];
      perSystem = {
        config,
        self',
        inputs',
        pkgs,
        system,
        lib,
        ...
      }: {
        rust-project.src = lib.sources.cleanSource ./.;
        rust-project.defaults.perCrate.crane.args.buildInputs = with pkgs; [pkg-config openssl];
        rust-project.crates.openfang-desktop.crane.args.buildInputs = with pkgs; [
          atk
          glib
          gtk3
          #gtk4 #try without
          openssl
          pkg-config
          webkitgtk_4_1
        ];

        packages.default = config.packages.openfang-cli;
        apps = {
          openfang-cli = {
            program = "${config.packages.openfang-cli}/bin/openfang";
            meta.description = "CLI tool for the OpenFang Agent OS";
          };
          openfang-desktop = {
            program = "${config.packages.openfang-desktop}/bin/openfang-desktop";
            meta.description = "Native desktop application for the OpenFang Agent OS (Tauri 2.0)";
          };
          default = config.apps.openfang-cli;
        };
      };
      flake = {
      };
    };
}
