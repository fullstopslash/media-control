---
story: 001-simplify-avoid
unit: 003-logic-cleanup
intent: 001-test-and-refactor
priority: Must
estimate: M
---

## Story: Simplify Avoid Command Logic

### Technical Story
**Description**: Restructure the avoid command to reduce nesting depth and make the 4 cases clearer. Extract shared patterns into helpers.
**Rationale**: The current avoid function has deeply nested if/else chains with duplicated logic between cases. A clearer structure makes bugs easier to spot and the code easier to extend.

### Acceptance Criteria
- [ ] All avoid E2E tests from unit 002 still pass
- [ ] No function in avoid.rs exceeds 4 levels of nesting
- [ ] Shared patterns (move + suppress, position pair lookup) are extracted
- [ ] Each avoidance case is clearly labeled and separated
- [ ] The duplicate "Case 3" comment (appears twice in current code) is resolved

### Technical Notes
- Consider an enum for avoidance cases with a match-based dispatch
- The current `media focused` check happens twice (single-workspace + multi-workspace) - could be unified
- `move_media_window` already exists as a helper - look for other extraction opportunities
- `calculate_target_position` is clean, keep as-is

### Dependencies
- All tests from 002-test-coverage must exist and pass
