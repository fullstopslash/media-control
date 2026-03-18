---
story: 004-edge-cases
unit: 002-test-coverage
intent: 001-test-and-refactor
priority: Should
estimate: M
---

## Story: Cross-Cutting Edge Cases

### Technical Story
**Description**: Test edge cases that span multiple modules: window matching priority, config resolution, suppress timing.
**Rationale**: These are the subtle bugs that only appear under specific conditions.

### Acceptance Criteria

**Window Matching:**
- [ ] Multiple media windows with same priority: first match wins
- [ ] pinned_only pattern: matches pinned OR fullscreen, rejects unpinned+non-fullscreen
- [ ] Invalid regex in non-strict mode: skipped, valid patterns still work
- [ ] Empty patterns list: no matches

**Config:**
- [ ] Position override with both class AND title regex: both must match
- [ ] Position override with only class: matches any title
- [ ] Position override with only title regex: matches any class
- [ ] resolve_position("0"): returns Some(0)
- [ ] resolve_position with negative value: returns correctly
- [ ] resolve_position with unknown name: returns None

**Suppress:**
- [ ] Timestamp exactly at suppress boundary: suppressed (< is strict)
- [ ] Timestamp 1ms past boundary: not suppressed
- [ ] Corrupt suppress file content: not suppressed (graceful fallback)
- [ ] Missing suppress file: not suppressed

### Dependencies
- 001-mock-infrastructure (003-test-context)
