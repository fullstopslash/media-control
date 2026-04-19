# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Bash-based media window control applet for Hyprland (Wayland compositor). Manages floating/pinned media windows (mpv, Picture-in-Picture, Jellyfin Media Player) with automatic avoidance, positioning, and Jellyfin server integration.

## Architecture

### Core Scripts

- **bash/media-control.sh** - Main entry point with subcommands:
  - `fullscreen` - Toggle fullscreen with focus restoration and pin state preservation
  - `move h|j|k|l` - Vim-style directional movement to preset positions
  - `close` - Graceful window closing (handles mpv/Jellyfin session cleanup)
  - `avoid` - Smart window repositioning to avoid overlap (called by trigger daemon)
  - `pin-and-float` - Toggle pinned floating mode with positioning
  - `mark-watched`, `mark-watched-and-stop`, `mark-watched-and-next` - Jellyfin integration
  - `chapter next|prev` - mpv chapter navigation

- **bash/media-avoider-trigger.sh** - Event-driven daemon listening to Hyprland socket events. Uses FIFO-based debouncing (15ms) to batch rapid events.

- **bash/jellyfin-control.sh** - Jellyfin server API client for session control (stop, mark watched, queue advancement). Uses credentials from `~/.config/jellyfin-mpv-shim/cred.json`.

### Key Patterns

**Hyprland IPC**: Direct socket communication via `socat` (66% faster than `hyprctl`):
```bash
_hypr_cmd() { echo -n "$1" | socat - "UNIX-CONNECT:$_HYPR_SOCKET" 2>/dev/null; }
```

**JSON processing**: Uses `jaq` (not `jq`) for all JSON parsing. Config uses `toml` CLI tool.

**Configuration**: Media window patterns and positions defined in `~/.config/hypr/media-windows.conf` (TOML format).

## Dependencies

- `socat` - Unix socket communication
- `jaq` - JSON processing (jq alternative)
- `toml` - TOML config parsing (toml-cli)
- `playerctl` - Optional, for mpv playback control
- `curl` - Jellyfin API calls

## Running

```bash
# Direct script execution
bash/media-control.sh <command> [args]

# Start avoidance daemon
bash/media-avoider-trigger.sh start
```

## Development Notes

- Scripts use `set -euo pipefail` for strict error handling
- Window matching uses regex patterns from config (class/title-based)
- Avoidance suppression prevents repositioning loops via timestamp file at `$XDG_RUNTIME_DIR/media-avoider-suppress`
