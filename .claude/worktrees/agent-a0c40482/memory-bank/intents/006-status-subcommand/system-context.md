---
intent: 006-status-subcommand
phase: inception
status: context-defined
updated: 2026-03-19T19:00:00Z
---

# Status Subcommand - System Context

## System Overview

The `status` command is a lightweight query tool that reads mpv playback state via IPC socket. No Hyprland or Jellyfin dependency — just direct mpv IPC. Designed for status bar integration and scripting.

## Context Diagram

```mermaid
C4Context
    title System Context - Status Subcommand

    Person(user, "User / Status Bar", "Queries playback state")
    System(mc, "media-control status", "Queries mpv IPC for playback properties")
    System_Ext(mpv, "mpv", "Media player with IPC socket")

    Rel(user, mc, "CLI invocation or polling")
    Rel(mc, mpv, "get_property commands via Unix socket")
```

## External Integrations

- **mpv IPC socket**: `get_property` commands for media-title, playback-time, duration, pause. Single connection, 4 commands, 4 responses.

## High-Level Constraints

- No Hyprland or Jellyfin dependency
- Fast: single attempt, no retry, 200ms timeout
- Exit code 0 = playing, 1 = not playing

## Key NFR Goals

- < 50ms response for status bar polling suitability
