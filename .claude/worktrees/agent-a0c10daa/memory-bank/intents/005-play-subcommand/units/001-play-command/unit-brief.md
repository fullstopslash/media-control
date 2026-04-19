---
unit: 001-play-command
intent: 005-play-subcommand
phase: inception
status: complete
created: 2026-03-19T18:00:00.000Z
updated: 2026-03-19T18:00:00.000Z
---

# Unit Brief: Play Command

## Purpose

Implement the `media-control play` subcommand that resolves a playback target, sends an IPC hint, gets resume position, and initiates playback via Jellyfin — replacing shim-play.sh.

## Scope

### In Scope
- 3 new Jellyfin API methods (global next-up, item resume ticks, play with resume)
- Multi-arg script-message IPC helper
- PlayConfig struct for config.toml `[play]` section
- play.rs command module with PlayTarget enum + orchestration
- CLI wiring in main.rs

### Out of Scope
- Daemon/background mode
- Queue building (shim handles advance)
- Interactive picker UI
- Hyprland keybinding changes (manual step after verification)

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Resolve `next-up` via global NextUp query | Must |
| FR-2 | Resolve `recent-pinchflat` via unwatched items query | Must |
| FR-3 | Resolve direct `<item-id>` passthrough | Must |
| FR-4 | Send IPC play-source hint (multi-arg) | Must |
| FR-5 | Get resume position (PlaybackPositionTicks) | Must |
| FR-6 | Find session + PlayNow with resume | Must |
| FR-7 | PlayConfig with pinchflat_library_id | Must |
| FR-8 | Error reporting (stderr + notify-send) | Must |

---

## Domain Concepts

### Key Entities
| Entity | Description | Attributes |
|--------|-------------|------------|
| PlayTarget | What to play | NextUp, RecentPinchflat, ItemId(String) |
| JellyfinClient | API client | address, token, user_id, device_id |
| PlayConfig | Play section config | pinchflat_library_id |

### Key Operations
| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| resolve_target | Map PlayTarget to item ID | target string | item_id |
| send_ipc_hint | Set play-source context | target type | () |
| get_resume | Fetch PlaybackPositionTicks | item_id | ticks (i64) |
| play_item | Find session + PlayNow | item_id, ticks | () |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 5 |
| Must Have | 5 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-jellyfin-methods | Add 3 Jellyfin API methods | Must | Planned |
| 002-multi-arg-ipc | Multi-arg script-message helper | Must | Planned |
| 003-play-config | PlayConfig struct | Must | Planned |
| 004-play-command | play.rs command module | Must | Planned |
| 005-cli-wiring | Wire into main.rs | Must | Planned |

---

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| 004-ipc-reliability / 001-ipc-hardening | Uses send_mpv_ipc_command for IPC hint |

### External Dependencies
| System | Purpose | Risk |
|--------|---------|------|
| Jellyfin API | Item resolution, session, PlayNow | Low — stable API |
| mpv IPC socket | Play-source hint | Low — uses hardened IPC from intent 004 |

---

## Technical Context

### Integration Points
| Integration | Type | Protocol |
|-------------|------|----------|
| Jellyfin Server | REST API | HTTPS with MediaBrowser auth |
| mpv IPC | Unix socket | JSON newline-delimited |

---

## Constraints

- Must reuse existing JellyfinClient (no new HTTP client)
- No new crate dependencies
- IPC hint before PlayNow (ordering guaranteed by latency differential)

---

## Success Criteria

### Functional
- [-] `media-control play next-up` plays first NextUp item <!-- tw:df0604b7-3b37-4290-86b3-dfa2e96c228f -->
- [-] `media-control play recent-pinchflat` plays most recent unwatched Pinchflat video <!-- tw:47d4896e-2db2-41af-b72f-89404ac1ce4d -->
- [-] `media-control play <item-id>` plays specific item <!-- tw:6c324fcb-a114-4887-9ed2-aa6cd33c8d32 -->
- [-] Playback resumes from last position <!-- tw:d1bf8c72-c90a-4c0b-aecd-06608e3c0ed8 -->
- [-] IPC hint arrives before PlayNow <!-- tw:479a35c0-d68f-4c2d-a8e1-0c4a54588950 -->
- [-] Errors show stderr + notify-send <!-- tw:214f2952-466e-406a-ac94-21ab5de54140 -->

### Non-Functional
- [-] Total latency < 200ms <!-- tw:d593b7e8-5d1d-4ec1-847c-8d60e0eba9ab -->
- [-] 3 HTTP requests per invocation (down from 6) <!-- tw:e6acf38f-b827-46d7-ab1f-c53ec0115eab -->

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 010-play-command | simple-construction-bolt | all 5 | Full play subcommand in one pass |

---

## Notes

~225 lines of new Rust code total. Small scope, high leverage — eliminates a fragile shell script from the critical playback path.
