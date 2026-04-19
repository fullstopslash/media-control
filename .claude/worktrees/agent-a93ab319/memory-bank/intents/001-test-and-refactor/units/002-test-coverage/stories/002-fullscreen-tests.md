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
- [ ] Enter fullscreen: focuses window, unpins if pinned, dispatches fullscreen 0 <!-- tw:b461df28-ef7a-4956-b12c-67079c0484d1 -->
- [ ] Exit fullscreen: exits fullscreen, restores pin, restores focus to previous window, repositions <!-- tw:6600bd09-95e9-44ce-954f-49317217153f -->
- [ ] Auto-pin: when always_pin is set and window is unpinned, pins instead of fullscreening <!-- tw:8d256807-ab61-4468-8855-275adaacfad0 -->
- [ ] Pin preservation: pinned window → enter fullscreen (unpin) → exit fullscreen (re-pin) <!-- tw:260a108d-1e9f-410e-b964-0fe4e1b00f35 -->
- [ ] PiP detection: Picture-in-Picture title triggers pin restoration <!-- tw:53993730-9bef-4f54-b044-1a2cbde2c21b -->
- [ ] Retry logic: if fullscreen state doesn't clear, retries up to MAX_FULLSCREEN_EXIT_ATTEMPTS <!-- tw:872189f1-eab6-432c-8658-3d9dde1b9c78 -->
- [ ] Edge: window disappears between retries (removed from clients list) <!-- tw:182b4937-8648-43b4-8c20-4ca6037cf694 -->
- [ ] Edge: no media window found → silent no-op <!-- tw:ce7790c2-97ac-44cb-b32c-bd3f27ca4094 -->
- [ ] Focus restoration: previous focus window is validated (mapped, not hidden) <!-- tw:58c1db53-dce8-4a70-8854-69bb3296f028 -->
- [ ] Focus restoration: falls back to most recent window if prev is invalid <!-- tw:42f916fe-f297-4778-a2bf-976322eec447 -->

### Dependencies
- 001-mock-infrastructure (all stories)
