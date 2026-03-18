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
- [ ] All 4 move directions tested
- [ ] Pin toggle on/off tested
- [ ] Close: mpv, jellyfin, PiP, default paths tested
- [ ] Focus: found and not-found tested
- [ ] Config edge cases: resolve_position(0), negative, unknown
- [ ] Window matching: multiple same-priority, pinned_only
- [ ] No flaky tests
