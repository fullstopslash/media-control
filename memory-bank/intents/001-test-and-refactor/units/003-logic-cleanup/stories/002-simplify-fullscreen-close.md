---
story: 002-simplify-fullscreen-close
unit: 003-logic-cleanup
intent: 001-test-and-refactor
priority: Must
estimate: M
---

## Story: Simplify Fullscreen and Close Commands

### Technical Story
**Description**: Clean up fullscreen exit logic (remove unused param, simplify retry) and deduplicate close command's killwindow branches.
**Rationale**: `exit_fullscreen` has an unused `_clients` parameter and complex retry nesting. `close_window_gracefully` has two identical killwindow code paths.

### Acceptance Criteria

**Fullscreen:**
- [ ] `_clients` parameter removed from `exit_fullscreen`
- [ ] Retry loop simplified (consider early return pattern or loop with break conditions)
- [ ] `exit_fullscreen_mode` calls `exit_fullscreen` with fewer parameters
- [ ] All fullscreen E2E tests pass
- [ ] `#[allow(clippy::too_many_arguments)]` removed (fewer args now)

**Close:**
- [ ] Jellyfin and default killwindow branches merged into single fallthrough
- [ ] mpv-specific logic remains distinct (it has different behavior)
- [ ] Firefox PiP rejection remains distinct
- [ ] All close E2E tests pass

### Dependencies
- All tests from 002-test-coverage must exist and pass
