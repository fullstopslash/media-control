---
stage: test
bolt: 011-status-command
created: 2026-03-19T19:00:00Z
---

## Test Report: status-command

### Summary

- **Tests**: 194/194 passed
- **New tests added**: 5 (4 unit + 1 doc-test)
- **Regressions**: 0

### Test Files

- [x] `crates/media-control-lib/src/commands/status.rs` - Time formatting tests (4) <!-- tw:b60137c1-7636-4219-a07a-4e935500e3c4 -->
- [x] `crates/media-control-lib/src/commands/mod.rs` - query_mpv_property doc-test (compile check) <!-- tw:90cc9c27-df8c-45b4-8f21-090c58f10b6c -->

### New Tests

| Test | Type | Covers |
|------|------|--------|
| `format_time_zero` | Unit | 0 seconds → "0:00" |
| `format_time_seconds_only` | Unit | 45.7s → "0:45" |
| `format_time_minutes_and_seconds` | Unit | 754.2s → "12:34" |
| `format_time_over_an_hour` | Unit | 3661s → "61:01" |
| `query_mpv_property` doc-test | Compile | Verifies function signature compiles |

### Acceptance Criteria Validation

- ✅ Human-readable output format implemented (title, position MM:SS / MM:SS, paused)
- ✅ JSON output with all fields (playing, title, position, duration, paused)
- ✅ Exit 0 when playing, exit 1 when not (routed in main.rs)
- ✅ Not-playing emits `{"playing":false}` with --json
- ✅ `cargo clippy` clean
- ✅ `cargo test` 194/194 pass

### Notes

Live testing (actual mpv playing) is manual. Unit tests cover time formatting. The query_mpv_property function reuses the same socket validation pattern as send_mpv_ipc_command.
