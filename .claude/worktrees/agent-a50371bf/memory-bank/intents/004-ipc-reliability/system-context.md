---
intent: 004-ipc-reliability
phase: inception
status: context-defined
updated: 2026-03-19T12:00:00Z
---

# IPC Reliability - System Context

## System Overview

media-control is a CLI tool that sends commands to mpv (via jellyfin-mpv-shim) over Unix domain sockets. The IPC path is: keypress → Hyprland binding → media-control CLI → mpv IPC socket → jellyfin-mpv-shim script-message handler.

## Context Diagram

```mermaid
C4Context
    title System Context - IPC Reliability

    Person(user, "User", "Presses keybindings in Hyprland")
    System(mc, "media-control", "Rust CLI sending IPC commands")
    System_Ext(mpv, "mpv", "Media player with IPC socket")
    System_Ext(shim, "jellyfin-mpv-shim", "Python shim controlling mpv, handles script-messages")
    System_Ext(hypr, "Hyprland", "Wayland compositor dispatching keybindings")
    System_Ext(notifyd, "notify-send", "Desktop notification daemon")

    Rel(user, hypr, "Keypress")
    Rel(hypr, mc, "Executes CLI")
    Rel(mc, mpv, "JSON over Unix socket")
    Rel(mpv, shim, "script-message dispatch")
    Rel(mc, notifyd, "Error notifications")
```

## External Integrations

- **mpv IPC socket**: Unix domain socket at `/tmp/mpvctl-jshim` (primary) or `/tmp/mpvctl0` (fallback). JSON protocol: `{"command":["script-message","<cmd>"]}\n` → `{"error":"success"}\n`
- **jellyfin-mpv-shim**: Registers IPC_COMMANDS dict in player.py. Handles mark-watched-next, skip-next, skip-prev, stop-and-clear, play-next-strategy.
- **notify-send**: Desktop notification for error feedback. Available on user's Arch/Hyprland setup.

## High-Level Constraints

- Must use tokio async runtime (existing Rust workspace)
- Socket paths are fixed by mpv `--input-ipc-server` flag
- mpv dies and respawns frequently; socket may be stale or missing during respawn window

## Key NFR Goals

- Command latency < 200ms happy path, < 800ms with retry
- 100% error visibility (no silent failures)
- Graceful handling of stale/missing/non-socket paths
