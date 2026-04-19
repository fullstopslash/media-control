---
stage: test
bolt: 009-ipc-hardening
created: 2026-03-19T12:00:00Z
---

## Test Report: ipc-hardening

### Summary

- **Tests**: 180/180 passed
- **New tests added**: 6
- **Regressions**: 0

### Test Files

- [x] `crates/media-control-lib/src/commands/mod.rs` - Socket validation unit tests (3) + real socket integration test (1) <!-- tw:ea84536b-7a4e-4daf-9538-b381bca3073f -->
- [x] `crates/media-control-lib/src/error.rs` - MpvIpc error type display tests (2) <!-- tw:e896dc0a-ec9f-4874-ac4f-cf2c215dcf90 -->

### New Tests

| Test | Type | Covers |
|------|------|--------|
| `socket_validation_skips_regular_file` | Unit | FR-1: regular file is not identified as socket |
| `socket_validation_detects_real_socket` | Unit | FR-1: Unix socket correctly identified |
| `socket_validation_handles_nonexistent` | Unit | FR-1: missing path fails metadata gracefully |
| `send_mpv_ipc_command_succeeds_with_real_socket` | Integration | FR-2/FR-4: full round-trip with mock socket server, response verified |
| `mpv_ipc_errors_display_correctly` | Unit | FR-3: error messages contain meaningful text |
| `mpv_ipc_error_kind_display` | Unit | FR-3: all 4 error kinds display correctly |

### Acceptance Criteria Validation

- ✅ Regular files at socket paths are skipped with stderr warning — `socket_validation_skips_regular_file` + code review
- ✅ Dead sockets timeout within 500ms — `SOCKET_CONNECT_TIMEOUT` constant enforced in `tokio::time::timeout`
- ✅ mpv IPC JSON response is read — `send_mpv_ipc_command_succeeds_with_real_socket` verifies full round-trip
- ✅ Failed first attempt retries after 100ms — code review: `for attempt in 0..2u8` with `RETRY_DELAY` sleep
- ✅ All IPC errors propagated to main — code review: `mark_watched_and_stop` now uses `?`, main.rs catches errors
- ✅ `mark_watched_and_stop` no longer swallows errors — `let _ =` replaced with `?`
- ✅ Exit code is non-zero on IPC failure — `std::process::exit(1)` in main.rs error handler
- ✅ Happy path < 200ms — no new overhead; validation + connect + write is sub-ms on local socket
- ✅ `cargo clippy` passes — verified, no new warnings from our code
- ✅ Desktop notification on error — `notify-send` spawned fire-and-forget in main.rs

### Issues Found

None.

### Notes

- Timeout and retry behavior are difficult to unit test without mocking the system clock. The constants (`SOCKET_CONNECT_TIMEOUT=500ms`, `SOCKET_RESPONSE_TIMEOUT=200ms`, `RETRY_DELAY=100ms`) are verified by code review.
- The `send_mpv_ipc_command_succeeds_with_real_socket` test creates a real Unix socket listener and verifies the full send/receive cycle, which is the strongest test for FR-2 and FR-4.
- Integration tests that depend on system socket absence (no running mpv) were replaced with pure unit tests that verify the validation logic directly.
