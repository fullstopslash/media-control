# media-control

Rust applet for managing floating media windows on Hyprland. Handles mpv, PiP, and Jellyfin Media Player with automatic avoidance, positioning, and Jellyfin server integration.

## Install

### Nix flake (flake-parts host)

Add to your flake's inputs:

```nix
inputs.media-control.url = "github:rain/media-control";
```

Then in your flake-parts host:

```nix
imports = [ inputs.media-control.flakeModules.default ];
```

This wires the overlay (so `pkgs.media-control` is available) and exposes `nixosModules.media-control` / `homeManagerModules.media-control`. Enable with:

```nix
services.media-control.enable = true;
```

### Nix flake (plain)

```nix
inputs.media-control.url = "github:rain/media-control";

# NixOS module:
imports = [ inputs.media-control.nixosModules.default ];

# or home-manager module:
imports = [ inputs.media-control.homeManagerModules.default ];

services.media-control.enable = true;
```

### mise

```sh
mise install
mise run install
```

Installs to `~/.local/bin`. Other tasks: `mise run build`, `lint`, `test`, `check`, `fmt`, `clean`, `dev`.

### Cargo (manual)

```sh
cargo build --release
install -m755 target/release/media-control{,-daemon} ~/.local/bin/
```

## Usage

```sh
media-control fullscreen              # toggle fullscreen
media-control move h|j|k|l            # move window directionally
media-control close                   # close media window
media-control pin-and-float           # pin and float current window
media-control mark-watched            # mark current Jellyfin item watched
media-control chapter next|prev       # chapter navigation
media-control-daemon foreground       # start avoidance daemon
```

## Configuration

`~/.config/hypr/media-windows.conf` (TOML)

## License

See license.md file for details.
