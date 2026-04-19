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
- [ ] All avoid E2E tests from unit 002 still pass <!-- tw:a4231b1f-2915-4c79-97b4-cfcd5b6dcf43 -->
- [ ] No function in avoid.rs exceeds 4 levels of nesting <!-- tw:09c372a2-87e7-4e13-8a49-4bd78a9c6345 -->
- [ ] Shared patterns (move + suppress, position pair lookup) are extracted <!-- tw:62da9625-a0d7-4adc-9a5a-9679bb62dd8c -->
- [ ] Each avoidance case is clearly labeled and separated <!-- tw:048dbcda-ebe4-4aca-951b-f7ecae5bed84 -->
- [ ] The duplicate "Case 3" comment (appears twice in current code) is resolved <!-- tw:bd83bd8d-186d-4218-ae24-f46520ad7984 -->

### Technical Notes
- Consider an enum for avoidance cases with a match-based dispatch
- The current `media focused` check happens twice (single-workspace + multi-workspace) - could be unified
- `move_media_window` already exists as a helper - look for other extraction opportunities
- `calculate_target_position` is clean, keep as-is

### Dependencies
- All tests from 002-test-coverage must exist and pass
