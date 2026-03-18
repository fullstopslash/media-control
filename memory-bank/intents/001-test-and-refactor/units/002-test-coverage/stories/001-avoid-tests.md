---
story: 001-avoid-tests
unit: 002-test-coverage
intent: 001-test-and-refactor
priority: Must
estimate: L
---

## Story: Avoid Command E2E and Edge Cases

### Technical Story
**Description**: Test all 4 avoidance cases end-to-end through the mock, plus edge cases.
**Rationale**: Avoid is the most complex command with 4 distinct code paths. It runs on every window event via the daemon.

### Acceptance Criteria
- [ ] Case 1 (single-workspace, non-media focused): media window moves to primary position
- [ ] Case 1: window already at primary position is not moved
- [ ] Case 2 (single-workspace, media focused/mouseover): toggles between primary and secondary positions
- [ ] Case 2: restores focus to previous window after toggle
- [ ] Case 2: skips if no previous window (empty workspace with pinned media)
- [ ] Case 3 (multi-workspace, media focused): geometry-based avoidance moves window away from overlap
- [ ] Case 3 (multi-workspace, non-media focused): geometry overlap detection and repositioning
- [ ] Case 4 (fullscreen non-media): media windows moved out of the way
- [ ] Edge: wide window threshold exactly at boundary
- [ ] Edge: suppression prevents avoid when timestamp is recent
- [ ] Edge: no focused window returns early
- [ ] Edge: no media windows on monitor returns early
- [ ] Edge: position overrides applied correctly per focused class/title

### Dependencies
- 001-mock-infrastructure (all stories)
