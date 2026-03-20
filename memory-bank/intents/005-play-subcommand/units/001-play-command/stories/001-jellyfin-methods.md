---
id: 001-jellyfin-methods
unit: 001-play-command
intent: 005-play-subcommand
status: complete
priority: must
created: 2026-03-19T18:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 001-jellyfin-methods

## User Story

**As a** media-control developer
**I want** 3 new Jellyfin API methods on JellyfinClient
**So that** the play subcommand can resolve items and initiate playback with resume

## Acceptance Criteria

- [ ] **Given** NextUp items exist, **When** `get_global_next_up()` is called, **Then** it returns the first item's ID <!-- tw:cc16a8ca-8d23-4f4a-9c10-070a0688b963 -->
- [ ] **Given** no NextUp items, **When** `get_global_next_up()` is called, **Then** it returns None <!-- tw:659e46e1-f360-41db-b1d8-8f3ce0d30f02 -->
- [ ] **Given** an item with resume position, **When** `get_item_resume_ticks(id)` is called, **Then** it returns the PlaybackPositionTicks value <!-- tw:7de5bb5d-8583-4bf8-870d-9fff345cda9f -->
- [ ] **Given** an item never played, **When** `get_item_resume_ticks(id)` is called, **Then** it returns 0 <!-- tw:7eb8fc74-ce40-418f-a1de-e4291c4b7e96 -->
- [ ] **Given** a valid session and item, **When** `play_item_with_resume(session, item, ticks)` is called with non-zero ticks, **Then** PlayNow includes StartPositionTicks <!-- tw:b697df0b-2a12-4885-8e2d-64ae0e1d1b2f -->
- [ ] **Given** ticks == 0, **When** `play_item_with_resume()` is called, **Then** PlayNow omits StartPositionTicks <!-- tw:e94e09b0-ab21-43c2-a548-fa70fe861f2d -->

## Technical Notes

- `get_global_next_up()`: `GET /Shows/NextUp?UserId={}&Limit=1` — differs from existing `get_next_up()` which is per-series
- `get_item_resume_ticks()`: `GET /Users/{}/Items/{}` → `UserData.PlaybackPositionTicks`
- `play_item_with_resume()`: Like existing `play_item()` but appends `&StartPositionTicks={}` when non-zero
- New deserialization structs: `ItemDetail { id, user_data: Option<UserData> }`, `UserData { playback_position_ticks }`

## Dependencies

### Requires
- None (extends existing JellyfinClient)

### Enables
- 004-play-command (uses all 3 methods)
