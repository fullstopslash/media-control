# media-control

Rust applet for managing floating media windows on Hyprland. Handles mpv, PiP, and Jellyfin Media Player with automatic avoidance, positioning, and Jellyfin server integration.

## Install

```sh
cargo build --release
```

Add `target/release/media-control` and `target/release/media-control-daemon` to your PATH.

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
