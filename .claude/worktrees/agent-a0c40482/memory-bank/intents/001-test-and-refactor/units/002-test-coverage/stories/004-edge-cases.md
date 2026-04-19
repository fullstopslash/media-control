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
- [ ] Multiple media windows with same priority: first match wins <!-- tw:d85d831d-5587-47cf-90af-0e3cb1c7f4dd -->
- [ ] pinned_only pattern: matches pinned OR fullscreen, rejects unpinned+non-fullscreen <!-- tw:03efa945-c02c-4a97-a75c-39c229363d33 -->
- [ ] Invalid regex in non-strict mode: skipped, valid patterns still work <!-- tw:efb29405-ecc3-4484-80c0-b2448fd06dac -->
- [ ] Empty patterns list: no matches <!-- tw:1c3999f5-4188-4693-b90e-db294fd3c040 -->

**Config:**
- [ ] Position override with both class AND title regex: both must match <!-- tw:27df6607-6b7e-47ad-a35e-87f1890d84c5 -->
- [ ] Position override with only class: matches any title <!-- tw:6b1eae52-da47-4988-bdbb-a69e6ec094f0 -->
- [ ] Position override with only title regex: matches any class <!-- tw:06edacaf-8eca-440c-b9e2-889bd6835783 -->
- [ ] resolve_position("0"): returns Some(0) <!-- tw:92866d02-56e7-4201-b68a-f80ab5ba24b4 -->
- [ ] resolve_position with negative value: returns correctly <!-- tw:54dd99ef-d654-4f1a-ab89-6f4654cff88c -->
- [ ] resolve_position with unknown name: returns None <!-- tw:3f0ae3b7-7a81-4a1a-b7a8-42ed0767d840 -->

**Suppress:**
- [ ] Timestamp exactly at suppress boundary: suppressed (< is strict) <!-- tw:3c45cb5c-fc17-46ff-acb3-e4fe0e60800e -->
- [ ] Timestamp 1ms past boundary: not suppressed <!-- tw:f8b74106-7e41-4dff-8ef3-43a203c142da -->
- [ ] Corrupt suppress file content: not suppressed (graceful fallback) <!-- tw:95fedec6-e481-4b8f-97db-d06564d96489 -->
- [ ] Missing suppress file: not suppressed <!-- tw:02d40112-e484-4b3f-b578-b19afafaa9c5 -->

### Dependencies
- 001-mock-infrastructure (003-test-context)
