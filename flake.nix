{
  description = "Media window control for Hyprland";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ {
    self,
    nixpkgs,
    flake-parts,
    crane,
    rust-overlay,
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux"];

      perSystem = {
        system,
        lib,
        ...
      }: let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [(import rust-overlay)];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        runtimeDeps = with pkgs; [playerctl libnotify];

        commonArgs = {
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;
          pname = "media-control";
          version = "0.1.4";

          buildInputs = [];
          nativeBuildInputs = with pkgs; [pkg-config makeWrapper];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        media-control = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;

            postInstall = ''
              for bin in $out/bin/*; do
                wrapProgram "$bin" \
                  --prefix PATH : ${lib.makeBinPath runtimeDeps}
              done

              mkdir -p $out/lib/systemd/user
              substitute ${./systemd/media-control-daemon.service} \
                $out/lib/systemd/user/media-control-daemon.service \
                --replace '%h/.cargo/bin/media-control-daemon' "$out/bin/media-control-daemon"
              cp ${./systemd/media-control-daemon.socket} \
                $out/lib/systemd/user/media-control-daemon.socket
            '';
          });
      in {
        packages = {
          default = media-control;
          inherit media-control;
        };

        devShells.default = craneLib.devShell {
          packages = with pkgs;
            [
              rust-analyzer
              cargo-watch
              cargo-edit
            ]
            ++ runtimeDeps;
        };

        checks = {
          inherit media-control;

          media-control-clippy = craneLib.cargoClippy (commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--workspace --all-targets -- --deny warnings";
            });

          media-control-tests = craneLib.cargoTest (commonArgs
            // {
              inherit cargoArtifacts;
              cargoTestExtraArgs = "--workspace";
            });

          media-control-fmt = craneLib.cargoFmt {
            inherit (commonArgs) src pname version;
          };
        };

        formatter = pkgs.alejandra;
      };

      flake = {
        overlays.default = final: _prev: {
          media-control = self.packages.${final.stdenv.hostPlatform.system}.default;
        };

        nixosModules.default = {
          config,
          lib,
          pkgs,
          ...
        }: let
          cfg = config.services.media-control;
        in {
          options.services.media-control = {
            enable = lib.mkEnableOption "media-control daemon for Hyprland";

            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              defaultText = lib.literalExpression "media-control.packages.\${system}.default";
              description = "media-control package to use";
            };

            users = lib.mkOption {
              type = lib.types.listOf lib.types.str;
              default = [];
              example = ["alice"];
              description = ''
                Users for whom the media-control-daemon user service is enabled.
                Each listed user gets a `media-control-daemon.service` user unit
                wired into `hyprland-session.target`.
              '';
            };
          };

          config = lib.mkIf cfg.enable {
            environment.systemPackages = [cfg.package];

            systemd.user.services.media-control-daemon = {
              description = "Media Control Daemon for Hyprland";
              partOf = ["graphical-session.target"];
              after = ["graphical-session.target"];
              unitConfig.ConditionEnvironment = "HYPRLAND_INSTANCE_SIGNATURE";
              serviceConfig = {
                Type = "simple";
                ExecStart = "${cfg.package}/bin/media-control-daemon foreground";
                Restart = "on-failure";
                RestartSec = 5;
                Environment = ["RUST_LOG=media_control=info"];
                NoNewPrivileges = true;
                ProtectSystem = "strict";
                ProtectHome = "read-only";
                ReadWritePaths = "%t";
              };
              wantedBy = lib.mkIf (cfg.users != []) ["hyprland-session.target"];
            };
          };
        };

        homeManagerModules.default = {
          config,
          lib,
          pkgs,
          ...
        }: let
          cfg = config.services.media-control;
        in {
          options.services.media-control = {
            enable = lib.mkEnableOption "media-control daemon";

            package = lib.mkOption {
              type = lib.types.package;
              default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
              defaultText = lib.literalExpression "media-control.packages.\${system}.default";
              description = "media-control package to use";
            };
          };

          config = lib.mkIf cfg.enable {
            home.packages = [cfg.package];

            systemd.user.services.media-control-daemon = {
              Unit = {
                Description = "Media Control Daemon for Hyprland";
                PartOf = ["graphical-session.target"];
                After = ["graphical-session.target"];
                ConditionEnvironment = "HYPRLAND_INSTANCE_SIGNATURE";
              };
              Service = {
                Type = "simple";
                ExecStart = "${cfg.package}/bin/media-control-daemon foreground";
                Restart = "on-failure";
                RestartSec = 5;
                Environment = "RUST_LOG=media_control=info";
              };
              Install.WantedBy = ["hyprland-session.target"];
            };
          };
        };

        flakeModules.default = _: {
          perSystem = {system, ...}: {
            _module.args.pkgs = import nixpkgs {
              inherit system;
              overlays = [self.overlays.default];
              config.allowUnfree = true;
            };
          };

          flake.nixosModules.media-control = self.nixosModules.default;
          flake.homeManagerModules.media-control = self.homeManagerModules.default;
        };
      };
    };
}
