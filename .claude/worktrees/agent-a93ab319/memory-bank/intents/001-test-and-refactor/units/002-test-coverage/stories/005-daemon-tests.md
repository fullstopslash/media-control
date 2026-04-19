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
- [ ] Debounce: events within debounce_ms window are coalesced (only one avoid triggered) <!-- tw:f279c0b0-7658-4c6b-ab26-4866a9f4d300 -->
- [ ] Debounce: events after debounce_ms trigger a new avoid <!-- tw:6fd867f7-8494-4d49-bf7a-fa4611a83bc2 -->
- [ ] Event filtering: only workspace/activewindow/movewindow/openwindow/closewindow/swapwindow trigger avoid <!-- tw:908141ce-2b37-4ac3-ae89-a95bea0eec0b -->
- [ ] Event filtering: other events (e.g., monitoradded) are ignored <!-- tw:07702e4d-7662-4be3-9599-3d4bd1809bad -->
- [ ] CommandContext reuse: context is created once, not per-event (verify by checking mock connection count or similar) <!-- tw:c1ba5038-52e5-4166-9627-aae70d9d71da -->

### Technical Notes
- These may need to test the debounce logic in isolation rather than the full event loop
- Extract debounce logic into a testable function if needed during cleanup

### Dependencies
- 001-mock-infrastructure (all stories)
