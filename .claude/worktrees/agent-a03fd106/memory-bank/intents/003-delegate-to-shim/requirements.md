---
intent: 003-delegate-to-shim
phase: inception
status: complete
created: 2026-03-18T22:00:00Z
updated: 2026-03-18T22:00:00Z
---

# Requirements: Delegate to Shim

## Intent Overview

Replace media-control's mark-watched-and-next strategy system with a thin mpv IPC call to the jellyfin-mpv-shim fork, which now handles advancement natively. Remove the now-redundant strategy code.

## Functional Requirements

### FR-1: Replace mark-watched-and-next with mpv IPC
- Send `{"command":["keypress","ctrl+n"]}` to mpv IPC socket
- Socket path: try $MPV_IPC_SOCKET, /tmp/mpvctl-jshim, /tmp/mpvctl0
- <5ms response (no HTTP calls)
- Priority: Must

### FR-2: Remove next-episode strategy system
- Delete strategy engine, library detection, NextEpisodeConfig from config.rs, mark_watched.rs, jellyfin.rs
- Remove [[next_episode.rules]] from config.toml
- Remove related tests
- Priority: Must

### FR-3: Keep mark-watched standalone
- mark-watched and mark-watched-and-stop unchanged (Jellyfin API)
- Priority: Must

### FR-4: Update Hyprland keybinding
- $mainMod CTRL, period calls media-control mark-watched-and-next (now thin IPC)
- Priority: Must

### FR-5: Align socket paths
- mark-watched-and-next uses same socket discovery as chapter command
- Priority: Should
