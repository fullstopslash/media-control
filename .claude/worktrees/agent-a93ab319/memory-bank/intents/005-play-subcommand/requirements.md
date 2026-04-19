---
intent: 005-play-subcommand
phase: inception
status: complete
created: 2026-03-19T18:00:00.000Z
updated: 2026-03-19T18:00:00.000Z
---

# Requirements: Play Subcommand

## Intent Overview

Replace `shim-play.sh` (6 serial curl calls + python JSON parsing, ~1-2s) with a native Rust `media-control play` subcommand (~50-100ms). Reuses existing JellyfinClient, session discovery, and mpv IPC infrastructure.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Sub-200ms playback initiation | Keypress to PlayNow < 200ms | Must |
| Replace fragile shell script | shim-play.sh deleted, keybindings use media-control | Must |
| Resume position support | Playback resumes from last position | Must |
| Configurable library ID | Pinchflat library ID from config.toml, not hardcoded | Must |

---

## Functional Requirements

### FR-1: Resolve item — `next-up`
- **Description**: Query Jellyfin `GET /Shows/NextUp?UserId={}&Limit=1` for the global next-up item (no series filter)
- **Acceptance Criteria**: Returns first NextUp item ID, or errors "No next-up item found"
- **Priority**: Must

### FR-2: Resolve item — `recent-pinchflat`
- **Description**: Query Jellyfin for most recent unwatched video in the Pinchflat library using existing `get_unwatched_items()`
- **Acceptance Criteria**: Returns first unwatched item ID from configured library, or errors "No unwatched Pinchflat videos found"
- **Priority**: Must

### FR-3: Resolve item — direct `<item-id>`
- **Description**: Accept a Jellyfin item ID directly, no API call needed
- **Acceptance Criteria**: Passes ID through to PlayNow; warns if format looks unexpected
- **Priority**: Must

### FR-4: Send IPC play-source hint
- **Description**: Before PlayNow, send `set-play-source` via mpv IPC to tell the shim which context this playback belongs to. Requires multi-arg script-message support.
- **Acceptance Criteria**: IPC hint arrives before PlayNow; failure is non-fatal (warn and continue)
- **Priority**: Must

### FR-5: Get resume position
- **Description**: Query `GET /Users/{}/Items/{item_id}` for `UserData.PlaybackPositionTicks`, pass to PlayNow as `StartPositionTicks`
- **Acceptance Criteria**: Playback resumes from last position; 0 ticks if never played
- **Priority**: Must

### FR-6: Find session and send PlayNow
- **Description**: Use existing `find_mpv_session()` + new `play_item_with_resume()` with optional StartPositionTicks
- **Acceptance Criteria**: Playback starts in shim; errors "Shim not connected" if no session
- **Priority**: Must

### FR-7: Library ID configuration
- **Description**: Add `[play]` section to config.toml with `pinchflat_library_id`
- **Acceptance Criteria**: Config loads; required only for `recent-pinchflat` target
- **Priority**: Must

### FR-8: Error reporting
- **Description**: Errors produce stderr message + notify-send (same pattern as existing main.rs)
- **Acceptance Criteria**: Specific messages for each failure mode
- **Priority**: Must

---

## Non-Functional Requirements

### Performance
| Requirement | Metric | Target |
|-------------|--------|--------|
| Total latency | Keypress to PlayNow | < 200ms |
| HTTP requests | Per invocation | 3 (down from 6) |

---

## Constraints

### Technical Constraints
- Must reuse existing JellyfinClient, credentials, session discovery
- IPC hint must arrive before PlayNow (guaranteed by local IPC < 1ms vs Jellyfin round-trip 50-200ms)
- No new crate dependencies needed

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| Shim is running when play is invoked | PlayNow fails | Error with "Shim not connected" + notify-send |
| Pinchflat library ID is stable | recent-pinchflat fails | Config-driven, easy to update |

---

## Open Questions

| Question | Owner | Due Date | Resolution |
|----------|-------|----------|------------|
| None | — | — | Spec is complete |
