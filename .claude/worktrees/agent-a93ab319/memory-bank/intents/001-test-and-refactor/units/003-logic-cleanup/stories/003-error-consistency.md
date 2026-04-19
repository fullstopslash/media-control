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
- [ ] No `.map_err()` closures that replicate `From<HyprlandError>` behavior <!-- tw:be7db135-30de-4aae-b260-ad4fd45c7759 -->
- [ ] `chapter.rs`: WindowNotFound for missing mpv socket replaced with a more descriptive error (e.g., `Io` or a new `MpvSocketNotFound` variant) <!-- tw:158ab104-9dd3-4d63-82ce-a18686e73093 -->
- [ ] All error messages are actionable (user can understand what to do) <!-- tw:28eb52c1-7cde-427b-aa72-23031a164573 -->
- [ ] `From` impls cover all cross-module conversion paths <!-- tw:fd6e118b-3fdb-4d9e-9019-161e5423777e -->
- [ ] All tests pass <!-- tw:bc654cf9-7ec6-464b-821e-8f8d7e268ed2 -->

### Technical Notes
- Run `grep -r "map_err" crates/media-control-lib/src/commands/` to find any remaining
- The `MediaControlError` enum may need a small addition for mpv socket errors

### Dependencies
- All tests from 002-test-coverage must exist and pass
