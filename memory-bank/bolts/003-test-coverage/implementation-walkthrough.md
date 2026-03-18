---
stage: implement
bolt: 003-test-coverage
created: 2026-03-18T17:00:00Z
---

## Implementation Walkthrough: simple commands + edge cases

### Summary

Added 23 new tests across move_window (5), pin (4), close (5), focus (2), and config edge cases (7). Completes E2E coverage for all commands and exercises config boundary conditions.

### Completed Work

- [x] `crates/media-control-lib/src/commands/move_window.rs` - 5 E2E tests (4 directions + no-op)
- [x] `crates/media-control-lib/src/commands/pin.rs` - 4 E2E tests (toggle on, toggle off, fullscreen guard, no-op)
- [x] `crates/media-control-lib/src/commands/close.rs` - 5 E2E tests (jellyfin killwindow, PiP error, mpv no-kill, default killwindow, no-op)
- [x] `crates/media-control-lib/src/commands/focus.rs` - 2 E2E tests (found + not found)
- [x] `crates/media-control-lib/src/config.rs` - 7 edge case tests (resolve_position boundaries, override matching combinations)

### Key Decisions

- **Close mpv test**: Verifies killwindow is NOT dispatched (mpv uses playerctl path). Jellyfin/playerctl calls fail gracefully in test environment.
- **Close default test**: Adds a custom "vlc" pattern to config to test the default killwindow path with a non-standard media class.
- **Config override tests**: Cover all 3 combinations (class+title, class-only, title-only) plus the negative cases.

### Deviations from Plan

- Daemon debounce tests deferred - the debounce logic is tightly coupled to the event loop and would need extraction to test in isolation. This is better handled during the logic cleanup bolts.

### Dependencies Added

None
