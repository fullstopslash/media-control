---
intent: 001-test-and-refactor
phase: inception
status: complete
created: 2026-03-18T12:30:00Z
updated: 2026-03-18T13:00:00Z
---

# Requirements: Test and Refactor

## Intent Overview

Comprehensive testing infrastructure, end-to-end test coverage, and implementation cleanup for the media-control project. Verify all functionality is correct and bug-free, build mock infrastructure for Hyprland IPC testing, and clean up complex logic without breaking behavior.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Prove all commands work correctly | Every command has end-to-end tests with mock IPC | Must |
| Catch edge case bugs | Known boundary conditions have tests | Must |
| Cleaner, more maintainable code | Reduced nesting, no duplication, consistent error handling | Must |
| Robust daemon | Daemon handles disconnects and rapid events correctly | Should |

---

## Functional Requirements

### FR-1: Mock Hyprland IPC Infrastructure
- **Description**: Build a mock Hyprland socket server that simulates request/response IPC for `j/clients`, `j/activewindow`, `j/monitors`, `dispatch`, `keyword`, and `[[BATCH]]` commands. Tests can configure canned responses per command.
- **Acceptance Criteria**:
  - Mock server binds to a temp Unix socket and responds to all command types used by `HyprlandClient`
  - `HyprlandClient` can be constructed with the mock socket path
  - Tests can set up expected responses before each test
  - Mock validates command format (batch prefix, dispatch prefix)
- **Priority**: Must
- **Related Stories**: TBD

### FR-2: End-to-End Command Tests
- **Description**: Test each command (`fullscreen`, `move`, `close`, `focus`, `avoid`, `pin-and-float`, `chapter`, `mark-watched*`) end-to-end through `CommandContext` using the mock IPC.
- **Acceptance Criteria**:
  - Each command has at least one happy-path test verifying correct Hyprland commands dispatched
  - Each command has tests for "no media window found" case
  - `fullscreen` tests: enter, exit, pin restoration, retry logic, focus restoration
  - `avoid` tests: all 4 cases (single-workspace non-media, single-workspace mouseover, multi-workspace mouseover, geometry overlap)
  - `move` tests: all 4 directions produce correct movewindowpixel/resizewindowpixel commands
  - `pin-and-float` tests: toggle on, toggle off, fullscreen guard
  - `close` tests: mpv path, jellyfin path, firefox PiP rejection
- **Priority**: Must
- **Related Stories**: TBD

### FR-3: Edge Case Coverage
- **Description**: Test known edge cases and boundary conditions in window matching, avoidance logic, and state management.
- **Acceptance Criteria**:
  - `avoid`: wide window threshold boundary (exactly at threshold)
  - `avoid`: suppression timing edge (timestamp exactly at boundary)
  - `avoid`: empty workspace with only pinned media window
  - `fullscreen`: window disappears between retries
  - `fullscreen`: already-pinned window entering/exiting fullscreen preserves pin
  - `window matcher`: multiple media windows with same priority
  - `config`: position override with both class and title regex matching
  - `config`: resolve_position with edge values (0, negative, very large)
- **Priority**: Must
- **Related Stories**: TBD

### FR-4: Logic Cleanup and Simplification
- **Description**: Refactor complex command implementations for clarity and maintainability. Full rewrites allowed as long as behavior is preserved (verified by new tests).
- **Acceptance Criteria**:
  - `exit_fullscreen`: remove unused `_clients` parameter, simplify retry loop
  - `avoid`: extract duplicated patterns, reduce nesting depth
  - `close`: deduplicate the two identical `killwindow` branches (jellyfin + default)
  - All existing 98 unit + 3 integration + 17 doc-tests continue to pass
  - No behavioral changes to any command as observed by the user
- **Priority**: Must
- **Related Stories**: TBD

### FR-5: Error Handling Consistency
- **Description**: Ensure all commands use the `From` impl error conversion consistently. Fix semantically incorrect error variants.
- **Acceptance Criteria**:
  - No inline `.map_err()` closures that replicate what `From<HyprlandError>` already does
  - `chapter.rs`: `WindowNotFound` error for missing mpv socket replaced with appropriate variant
  - Error messages are actionable
- **Priority**: Should
- **Related Stories**: TBD

### FR-6: Daemon Robustness
- **Description**: Verify daemon handles connection loss, reconnection, and edge cases gracefully.
- **Acceptance Criteria**:
  - Test: daemon event loop handles Hyprland socket closing cleanly
  - Test: FIFO trigger works after writer disconnects and reconnects
  - Test: debounce logic correctly coalesces rapid events
  - Test: CommandContext is created once and reused
- **Priority**: Should
- **Related Stories**: TBD

---

## Non-Functional Requirements

### Performance
| Requirement | Metric | Target |
|-------------|--------|--------|
| IPC round-trip | Mock socket test latency | < 5ms per command |
| Avoid command | End-to-end with mock | < 10ms (no real I/O) |

### Reliability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Test suite | All tests pass | 100% green |
| Test coverage | Command logic paths | Every branch in avoid/fullscreen/pin tested |

### Maintainability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Cyclomatic complexity | Per function | No functions with >4 nesting levels |
| Code duplication | Across commands | No duplicated blocks >5 lines |

---

## Constraints

### Technical Constraints

- Mock infrastructure must not require a running Hyprland instance
- No new external dependencies for mocking (use tokio's `UnixListener` directly)
- Jellyfin HTTP mocking is explicitly out of scope
- Existing test helpers (`make_client`, `make_client_full`) should be reused/extended

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| Mock socket can faithfully simulate Hyprland responses | Tests pass but real behavior differs | Validate mock responses against real Hyprland JSON samples |
| Refactored logic preserves all edge case behavior | Regression bugs | Write tests BEFORE refactoring |

---

## Open Questions

| Question | Owner | Due Date | Resolution |
|----------|-------|----------|------------|
| None currently | - | - | - |
