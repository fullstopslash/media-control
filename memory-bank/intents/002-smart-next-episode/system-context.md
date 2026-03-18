---
intent: 002-smart-next-episode
phase: inception
status: context-defined
updated: 2026-03-18T20:00:00Z
---

# Smart Next Episode - System Context

## System Overview

Extends the existing mark-watched-and-next command with per-library strategy dispatch. The system queries Jellyfin APIs to determine the library context and select the next item based on user-configured rules.

## Context Diagram

```mermaid
C4Context
    title System Context - Smart Next Episode

    Person(user, "User", "Presses mark-watched-and-next keybinding")

    System(mc, "media-control CLI", "Resolves library, dispatches strategy, plays next item")

    System_Ext(jellyfin, "Jellyfin Server", "Sessions, Items, Ancestors, NextUp, Collections APIs")
    System_Ext(mpvshim, "jellyfin-mpv-shim", "Receives play commands via Jellyfin session control")
    System_Ext(config, "config.toml", "next_episode.rules with per-library strategies")

    Rel(user, mc, "mark-watched-and-next")
    Rel(mc, jellyfin, "GET /Sessions, GET /Items/{id}/Ancestors, GET /Shows/{id}/NextUp, GET /Users/{id}/Items, POST /Sessions/{id}/Command/Play")
    Rel(mc, config, "reads next_episode.rules")
    Rel(jellyfin, mpvshim, "sends play command to shim session")
```

## Jellyfin API Endpoints Used

| Endpoint | Purpose | Strategy |
|----------|---------|----------|
| `GET /Sessions` | Find active mpv session | All |
| `GET /Items/{id}/Ancestors` | Determine library for current item | All (library detection) |
| `GET /Shows/{seriesId}/NextUp` | Next unwatched episode in series | next-up |
| `GET /Users/{id}/Items?ParentId={lib}&IsPlayed=false&SortBy=DateCreated` | Unwatched items sorted by date | recent-unwatched |
| `GET /Users/{id}/Items?ParentId={lib}&IsPlayed=false&SortBy=Random` | Random unwatched item | random-unwatched |
| `GET /Items/{boxsetId}/Items` | Items in a collection | series-or-random |
| `POST /Sessions/{id}/Command/Play` | Tell shim to play an item | All |

## High-Level Constraints

- Must work with existing jellyfin-mpv-shim session control
- All API calls use existing credential/auth system
- Config extends existing TOML format
- Strategy errors must not prevent mark-watched from succeeding
