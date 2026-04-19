---
stage: test
bolt: 002-test-coverage
created: 2026-03-18T16:15:00Z
---

## Test Report: avoid + fullscreen E2E

### Summary

- **Tests**: 149/149 passed (full suite)
- **New tests in this bolt**: 19 (13 avoid + 6 fullscreen)
- **Flake check**: 5 consecutive runs, 0 flakes
- **Regressions**: 0

### Test Files

- [x] `crates/media-control-lib/src/commands/avoid.rs` - 15 tests (4 existing + 11 new E2E + 2 suppress) <!-- tw:ebab4630-e93d-4ca3-b414-5f637ebef537 -->
- [x] `crates/media-control-lib/src/commands/fullscreen.rs` - 10 tests (4 existing + 6 new E2E) <!-- tw:86a11cd2-8624-4f34-b746-6eb578b37748 -->
- [x] `crates/media-control-lib/src/test_helpers.rs` - 13 tests (enhanced with sequence support) <!-- tw:ba5c548c-40f0-4f9d-b661-3c14a43707d5 -->

### Avoid E2E Tests

| Test | Case | What it verifies |
|------|------|-----------------|
| avoid_case1_moves_media_to_primary | 1 | Media not at primary → moved to x_right/y_bottom |
| avoid_case1_skips_when_already_at_primary | 1 | Media at primary → no move dispatched |
| avoid_case1_skips_fullscreen_focused | 1 | Focused window fullscreen → early return |
| avoid_case2_toggles_to_secondary | 2 | Media focused at primary → toggles to secondary + focus restore |
| avoid_case2_no_previous_window_skips | 2 | No previous window → skips (empty workspace) |
| avoid_case3_geometry_overlap_moves_media | 3 | Multi-workspace overlap → geometry-based move |
| avoid_case3_no_overlap_skips | 3 | Multi-workspace no overlap → no move |
| avoid_no_focused_window_returns_early | - | No focus_history_id=0 → early return |
| avoid_no_media_windows_returns_early | - | No matching media → early return |
| avoid_case1_applies_position_override | 1 | Firefox class override → x_left/y_top |
| should_suppress_with_recent_timestamp | - | Recent timestamp is detected as suppressed |
| should_suppress_with_stale_timestamp | - | 0ms timeout never suppresses |

### Fullscreen E2E Tests

| Test | What it verifies |
|------|-----------------|
| fullscreen_enter_unpinned | focus + fullscreen batch (no unpin) |
| fullscreen_enter_pinned_unpins_first | focus + unpin + fullscreen batch |
| fullscreen_exit_restores_pin | Exit → re-pin dispatched |
| fullscreen_no_media_window_is_noop | Only j/clients fetch, no dispatches |
| fullscreen_auto_pin_when_always_pin_set | PiP window → pin instead of fullscreen |
| fullscreen_exit_restores_focus_to_previous | Exit → focus restored to firefox with no_warps |

### Acceptance Criteria Validation

- ✅ All 4 avoid cases have E2E tests
- ✅ Avoid edge cases: no focus, no media, suppression, fullscreen focused, position override
- ✅ Fullscreen enter (pinned + unpinned) tested
- ✅ Fullscreen exit with pin restore tested
- ✅ Fullscreen auto-pin for always_pin windows tested
- ✅ Fullscreen focus restoration tested
- ✅ No flaky tests (5 consecutive clean runs)
- ✅ All 149 tests pass (no regressions)

### Issues Found

None.
