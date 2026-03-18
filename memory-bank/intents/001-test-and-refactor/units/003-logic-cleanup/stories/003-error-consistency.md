---
story: 003-error-consistency
unit: 003-logic-cleanup
intent: 001-test-and-refactor
priority: Should
estimate: S
---

## Story: Error Handling Consistency Pass

### Technical Story
**Description**: Audit all command files for error handling patterns. Fix any remaining verbose conversions and semantically incorrect error variants.
**Rationale**: Most were cleaned up in the earlier refactor pass, but a final audit ensures nothing was missed and new patterns introduced by the test/refactor work are consistent.

### Acceptance Criteria
- [ ] No `.map_err()` closures that replicate `From<HyprlandError>` behavior
- [ ] `chapter.rs`: WindowNotFound for missing mpv socket replaced with a more descriptive error (e.g., `Io` or a new `MpvSocketNotFound` variant)
- [ ] All error messages are actionable (user can understand what to do)
- [ ] `From` impls cover all cross-module conversion paths
- [ ] All tests pass

### Technical Notes
- Run `grep -r "map_err" crates/media-control-lib/src/commands/` to find any remaining
- The `MediaControlError` enum may need a small addition for mpv socket errors

### Dependencies
- All tests from 002-test-coverage must exist and pass
