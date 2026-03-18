---
story: 002-fullscreen-tests
unit: 002-test-coverage
intent: 001-test-and-refactor
priority: Must
estimate: M
---

## Story: Fullscreen Command E2E and Edge Cases

### Technical Story
**Description**: Test fullscreen toggle including enter, exit with retry, pin state preservation, and focus restoration.
**Rationale**: Fullscreen has the most complex state management with retry logic, pin toggling, and cross-command interactions (calls avoid internally).

### Acceptance Criteria
- [ ] Enter fullscreen: focuses window, unpins if pinned, dispatches fullscreen 0
- [ ] Exit fullscreen: exits fullscreen, restores pin, restores focus to previous window, repositions
- [ ] Auto-pin: when always_pin is set and window is unpinned, pins instead of fullscreening
- [ ] Pin preservation: pinned window → enter fullscreen (unpin) → exit fullscreen (re-pin)
- [ ] PiP detection: Picture-in-Picture title triggers pin restoration
- [ ] Retry logic: if fullscreen state doesn't clear, retries up to MAX_FULLSCREEN_EXIT_ATTEMPTS
- [ ] Edge: window disappears between retries (removed from clients list)
- [ ] Edge: no media window found → silent no-op
- [ ] Focus restoration: previous focus window is validated (mapped, not hidden)
- [ ] Focus restoration: falls back to most recent window if prev is invalid

### Dependencies
- 001-mock-infrastructure (all stories)
