---
stage: plan
bolt: 003-test-coverage
created: 2026-03-18T16:30:00Z
---

## Implementation Plan: simple commands + edge cases + daemon

### Objective
Complete test coverage for move, pin, close, focus commands, cross-cutting edge cases, and daemon debounce logic.

### Deliverables
- E2E tests added to `move_window.rs`, `pin.rs`, `close.rs`, `focus.rs` test modules
- Edge case tests for config, window matching, suppress timing
- Daemon debounce unit tests

### Technical Approach

**Move tests** (4 directions + no-op): Straightforward - set up mock with mpv client, call move_window with each direction, assert movewindowpixel coordinates.

**Pin tests** (toggle on, toggle off, fullscreen guard): Set up different initial states (unpinned, pinned+floating, fullscreen), verify correct dispatch sequences.

**Close tests** (mpv, jellyfin, PiP, default):
- mpv: Can't easily test playerctl/Jellyfin calls, but can verify no killwindow is dispatched
- jellyfin: Verify killwindow dispatched
- PiP: Verify error returned
- default: Verify killwindow dispatched

**Focus tests** (found, not found): Verify focuswindow dispatched when found, Ok(false) when not.

**Edge cases**: Tested as unit tests within existing modules - config override matching, resolve_position boundaries, suppress timing.

**Daemon debounce**: Extract debounce logic test - verify events within window coalesce, events after window trigger new avoid. This is a unit test of the timing logic, not the full event loop.

### Acceptance Criteria
- [ ] All 4 move directions tested <!-- tw:b394d3d1-4bc4-46cd-a844-adb1e9b0d8c5 -->
- [ ] Pin toggle on/off tested <!-- tw:77c6ad6f-62b7-4eba-813c-a9b13cba815f -->
- [ ] Close: mpv, jellyfin, PiP, default paths tested <!-- tw:f155f9b0-a2a4-4aab-b203-80d59aaf3a4c -->
- [ ] Focus: found and not-found tested <!-- tw:7cb9b6e9-0d6a-46f9-b723-d0b8871bfa92 -->
- [ ] Config edge cases: resolve_position(0), negative, unknown <!-- tw:66185a08-79c6-4a68-9ebe-d72b50de6321 -->
- [ ] Window matching: multiple same-priority, pinned_only <!-- tw:d2e1f623-92e4-4d56-b8d2-118bf9cf4b00 -->
- [ ] No flaky tests <!-- tw:e0186bd3-f864-4fdb-b62b-c47e72429bd9 -->
