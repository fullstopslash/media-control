---
intent: 001-test-and-refactor
phase: inception
status: context-defined
updated: 2026-03-18T13:00:00Z
---

# Test and Refactor - System Context

## System Overview

media-control is a local desktop tool. It has no network-facing surface except outbound Jellyfin API calls. The "system" under test is the Rust codebase communicating with Hyprland via Unix sockets and optionally with a Jellyfin server via HTTP.

## Context Diagram

```mermaid
C4Context
    title System Context - media-control

    Person(user, "User", "Desktop user with Hyprland compositor")

    System(mc, "media-control CLI", "Commands: fullscreen, move, close, avoid, pin, focus, chapter, mark-watched")
    System(daemon, "media-control-daemon", "Listens to Hyprland events, triggers avoid")

    System_Ext(hyprland, "Hyprland", "Wayland compositor - Unix socket IPC")
    System_Ext(mpv, "mpv", "Media player - Unix socket IPC")
    System_Ext(jellyfin, "Jellyfin Server", "Media server - HTTP API")
    System_Ext(playerctl, "playerctl", "MPRIS playback control")
    System_Ext(config, "Config File", "~/.config/media-control/config.toml")

    Rel(user, mc, "Invokes via keybindings")
    Rel(mc, hyprland, "dispatch/query via .socket.sock")
    Rel(mc, mpv, "chapter commands via /tmp/mpvctl*")
    Rel(mc, jellyfin, "mark-watched, stop, next via HTTP")
    Rel(mc, playerctl, "stop via CLI")
    Rel(mc, config, "reads on startup")
    Rel(daemon, hyprland, "listens .socket2.sock events")
    Rel(daemon, mc, "triggers avoid command")
```

## External Integrations

- **Hyprland IPC** (.socket.sock): Request/response for window queries and dispatch commands. This is the primary integration to mock.
- **Hyprland Events** (.socket2.sock): Event stream for the daemon. Line-based protocol (`event>>data`).
- **mpv IPC** (/tmp/mpvctl*): JSON commands over Unix socket for chapter navigation.
- **Jellyfin HTTP API**: Session queries, mark-watched, playback control. Out of scope for mocking.
- **playerctl**: CLI invocation for mpv stop. Out of scope for mocking.
- **Config file**: TOML parsing, already well-tested.

## High-Level Constraints

- All testing must work without a running Hyprland instance
- Mock only the Hyprland request/response socket (not socket2 events)
- No new crate dependencies for mocking
- Jellyfin HTTP and playerctl remain tested only via unit/deserialization tests

## Key NFR Goals

- Every command logic path exercised by tests
- Refactored code verified by tests written BEFORE refactoring
- No regressions in existing 118 tests
