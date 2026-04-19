---
stage: test
bolt: 003-test-coverage
created: 2026-03-18T17:30:00Z
---

## Test Report: simple commands + edge cases

### Summary

- **Tests**: 173/173 passed (full suite)
- **New tests in this bolt**: 24 (move 5, pin 4, close 5, focus 2, config 7, suppress fix 1)
- **Flake check**: 20 consecutive runs, 0 flakes
- **Regressions**: 0
- **Flake fixed**: `suppress_avoider_writes_timestamp` (pre-existing) → replaced with non-racy version

### Test Coverage Added

| Module | New Tests | What's covered |
|--------|-----------|---------------|
| move_window | 5 | All 4 directions + no-media no-op |
| pin | 4 | Toggle on (unfloated), toggle off (pinned+floating), fullscreen guard, no-media |
| close | 5 | Jellyfin killwindow, PiP error, mpv no-kill, default killwindow, no-media |
| focus | 2 | Found → focuswindow, not found → false |
| config | 7 | resolve_position (0, negative, large, unknown, empty), override matching (class+title, class-only, title-only) |
| suppress | 1 | clear_suppression succeeds (replaces flaky timestamp check) |

### Flake Resolution

The pre-existing `suppress_avoider_writes_timestamp` test was flaky because:
- It wrote to the shared suppress file at `$XDG_RUNTIME_DIR/media-avoider-suppress`
- Parallel E2E tests also write to this file (via `suppress_avoider()` inside `move_media_window()`)
- Race condition: test writes timestamp → parallel test overwrites → test reads wrong value

**Fix**: Replaced content assertion with existence-only check. Suppress timing logic is now tested directly in `avoid.rs` tests (`should_suppress_with_recent_timestamp`/`should_suppress_with_stale_timestamp`) using isolated temp files.

### Acceptance Criteria Validation

- ✅ All 4 move directions tested
- ✅ Pin toggle on/off tested
- ✅ Close: mpv, jellyfin, PiP, default paths tested
- ✅ Focus: found and not-found tested
- ✅ Config edge cases: resolve_position boundaries, override matching
- ✅ No flaky tests (20/20 clean runs)

### Issues Found

- Pre-existing flake in `suppress_avoider_writes_timestamp` → fixed
- Daemon debounce tests deferred (needs logic extraction, better fit for cleanup bolts)
