---
story: 005-daemon-tests
unit: 002-test-coverage
intent: 001-test-and-refactor
priority: Should
estimate: S
---

## Story: Daemon Debounce and Lifecycle Tests

### Technical Story
**Description**: Test daemon event handling properties: debounce coalescing, event filtering, and clean shutdown.
**Rationale**: The daemon runs continuously and must handle rapid events without excess IPC calls.

### Acceptance Criteria
- [ ] Debounce: events within debounce_ms window are coalesced (only one avoid triggered)
- [ ] Debounce: events after debounce_ms trigger a new avoid
- [ ] Event filtering: only workspace/activewindow/movewindow/openwindow/closewindow/swapwindow trigger avoid
- [ ] Event filtering: other events (e.g., monitoradded) are ignored
- [ ] CommandContext reuse: context is created once, not per-event (verify by checking mock connection count or similar)

### Technical Notes
- These may need to test the debounce logic in isolation rather than the full event loop
- Extract debounce logic into a testable function if needed during cleanup

### Dependencies
- 001-mock-infrastructure (all stories)
