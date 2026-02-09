{
  description = "Media window control for Hyprland";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Use stable Rust
        rustToolchain = pkgs.rust-bin.stable.latest.default;

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Common args for crane builds
        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          pname = "media-control";
          version = "0.1.0";

          buildInputs = with pkgs; [
            # Add any native dependencies here
          ];

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];
        };

        # Build dependencies separately for caching
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Build the workspace
        media-control = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;

          # Install both binaries and systemd files
          postInstall = ''
            # Install systemd user service
            mkdir -p $out/lib/systemd/user
            substitute ${./systemd/media-control-daemon.service} $out/lib/systemd/user/media-control-daemon.service \
              --replace '%h/.cargo/bin/media-control-daemon' "$out/bin/media-control-daemon"

            # Install systemd socket unit
            substitute ${./systemd/media-control-daemon.socket} $out/lib/systemd/user/media-control-daemon.socket \
              --replace '%t/media-control-daemon.sock' '%t/media-control-daemon.sock'
          '';
        });

      in {
        packages = {
          default = media-control;
          media-control = media-control;
        };

        # Development shell
        devShells.default = craneLib.devShell {
          packages = with pkgs; [
            rust-analyzer
            cargo-watch
            cargo-edit
          ];
        };

        # Expose as NixOS/home-manager module
        # Users can add: services.media-control.enable = true;
      }
    ) // {
      # Home-manager module
      homeManagerModules.default = { config, lib, pkgs, ... }:
        let
          cfg = config.services.media-control;
        in {
          options.services.media-control = {
            enable = lib.mkEnableOption "media-control daemon";

            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.system}.default;
              description = "The media-control package to use";
            };
          };

          config = lib.mkIf cfg.enable {
            home.packages = [ cfg.package ];

            systemd.user.services.media-control-daemon = {
              Unit = {
                Description = "Media Control Daemon for Hyprland";
                PartOf = [ "graphical-session.target" ];
                After = [ "graphical-session.target" ];
                ConditionEnvironment = "HYPRLAND_INSTANCE_SIGNATURE";
              };

              Service = {
                Type = "simple";
                ExecStart = "${cfg.package}/bin/media-control-daemon foreground";
                Restart = "on-failure";
                RestartSec = 5;
              };

              Install = {
                WantedBy = [ "hyprland-session.target" ];
              };
            };
          };
        };
    };
}
