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
- [ ] `_clients` parameter removed from `exit_fullscreen` <!-- tw:67b0437f-74b0-4f6b-8ce9-44e94aed61b2 -->
- [ ] Retry loop simplified (consider early return pattern or loop with break conditions) <!-- tw:328bb08b-dcd9-4d04-b220-dbf1b0834158 -->
- [ ] `exit_fullscreen_mode` calls `exit_fullscreen` with fewer parameters <!-- tw:ba2e3b68-9e0f-4011-8968-7169bc5c3d63 -->
- [ ] All fullscreen E2E tests pass <!-- tw:7850ee0f-4067-4ee9-828a-0bf58819266f -->
- [ ] `#[allow(clippy::too_many_arguments)]` removed (fewer args now) <!-- tw:31478b29-bfc1-408e-baa8-4ade5f9c39e1 -->

**Close:**
- [ ] Jellyfin and default killwindow branches merged into single fallthrough <!-- tw:e6c2bf2f-0355-4ee8-b706-b0579dec42d0 -->
- [ ] mpv-specific logic remains distinct (it has different behavior) <!-- tw:8da315f8-d29d-4f5d-9433-411ccd2460a6 -->
- [ ] Firefox PiP rejection remains distinct <!-- tw:9596ac4c-fc98-40b5-8172-94ffc184f0f5 -->
- [ ] All close E2E tests pass <!-- tw:20832eab-a77b-4e34-85ba-1b96830317bc -->

### Dependencies
- All tests from 002-test-coverage must exist and pass
