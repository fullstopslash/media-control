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
- [ ] Case 1 (single-workspace, non-media focused): media window moves to primary position <!-- tw:1f79f94c-a687-434f-ae04-1dca87bd7c30 -->
- [ ] Case 1: window already at primary position is not moved <!-- tw:c1e3533c-354f-49e9-8e83-a5ba44af1cfc -->
- [ ] Case 2 (single-workspace, media focused/mouseover): toggles between primary and secondary positions <!-- tw:b3ee4273-3c38-47fd-8b80-500939c09f91 -->
- [ ] Case 2: restores focus to previous window after toggle <!-- tw:6deb5a22-b538-4cdf-b409-faddfdc1ff64 -->
- [ ] Case 2: skips if no previous window (empty workspace with pinned media) <!-- tw:67b8dcfe-4001-4d12-a9e9-fe7f14b38f6d -->
- [ ] Case 3 (multi-workspace, media focused): geometry-based avoidance moves window away from overlap <!-- tw:c69edc90-5961-4470-8e21-66fc09578a75 -->
- [ ] Case 3 (multi-workspace, non-media focused): geometry overlap detection and repositioning <!-- tw:dea021e2-1a71-476e-b44d-f0cbfc342c54 -->
- [ ] Case 4 (fullscreen non-media): media windows moved out of the way <!-- tw:2e670add-5b2a-4de3-bc2f-d9254f71f512 -->
- [ ] Edge: wide window threshold exactly at boundary <!-- tw:4766b689-3e43-46ce-8162-5a9ad4ed4999 -->
- [ ] Edge: suppression prevents avoid when timestamp is recent <!-- tw:384d8861-76bc-4829-b255-2283236ca423 -->
- [ ] Edge: no focused window returns early <!-- tw:1c59d5b5-9cc7-4b88-8f85-a549387b9c94 -->
- [ ] Edge: no media windows on monitor returns early <!-- tw:68001ae1-d53b-4a6f-b083-c204bd54f582 -->
- [ ] Edge: position overrides applied correctly per focused class/title <!-- tw:8fa06da6-f2bc-491d-b0e1-7e0adbd47f4b -->

### Dependencies
- 001-mock-infrastructure (all stories)
