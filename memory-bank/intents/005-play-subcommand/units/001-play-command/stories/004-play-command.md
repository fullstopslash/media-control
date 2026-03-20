---
id: 004-play-command
unit: 001-play-command
intent: 005-play-subcommand
status: complete
priority: must
created: 2026-03-19T18:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 004-play-command

## User Story

**As a** media-control user
**I want** a `media-control play <target>` command
**So that** I can start Jellyfin playback from a keybinding without the slow shell script

## Acceptance Criteria

- [ ] **Given** target "next-up", **When** play runs, **Then** it queries NextUp, sends IPC hint "nextup", gets resume ticks, and sends PlayNow <!-- tw:297ac190-a49d-4a23-b866-c366f6950574 -->
- [ ] **Given** target "recent-pinchflat", **When** play runs, **Then** it queries unwatched items, sends IPC hint "strategy", gets resume ticks, and sends PlayNow <!-- tw:3867e6b5-ac5b-421c-b840-3a420deb922c -->
- [ ] **Given** target is a hex item ID, **When** play runs, **Then** it skips resolution, sends IPC hint "strategy", gets resume ticks, and sends PlayNow <!-- tw:4cf51695-5632-466f-a5ce-3e92251cdfc1 -->
- [ ] **Given** IPC hint fails, **When** play runs, **Then** it warns but continues to PlayNow <!-- tw:85ce744a-7be1-4597-ac39-0d151190dffe -->
- [ ] **Given** no shim session, **When** play runs, **Then** it errors "Shim not connected" <!-- tw:b473ae8c-98c4-40ac-9d98-fe71eba2a7fc -->

## Technical Notes

- `PlayTarget` enum: `NextUp`, `RecentPinchflat`, `ItemId(String)`
- `PlayTarget::parse(s)` matches "next-up", "recent-pinchflat", or falls through to ItemId
- Orchestration: resolve → IPC hint → resume ticks → find session → PlayNow
- ~150 lines in `commands/play.rs`

## Dependencies

### Requires
- 001-jellyfin-methods (API methods)
- 002-multi-arg-ipc (IPC hint)
- 003-play-config (library ID)

### Enables
- 005-cli-wiring
