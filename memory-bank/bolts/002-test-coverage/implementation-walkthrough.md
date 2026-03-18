---
stage: implement
bolt: 002-test-coverage
created: 2026-03-18T16:00:00Z
---

## Implementation Walkthrough: avoid + fullscreen E2E tests

### Summary

Added 16 E2E tests for the avoid command (11 tests covering all 4 cases + edge cases) and fullscreen command (6 tests covering enter/exit/pin-restore/auto-pin/focus-restore). Enhanced MockHyprland with response sequence support for retry simulation.

### Structure Overview

Tests are co-located in each command's `mod tests` block. Mock infrastructure enhanced with sequence support for fullscreen retry testing. All tests use `suppress_ms: 0` config to avoid race conditions from the shared suppress file.

### Completed Work

- [x] `crates/media-control-lib/src/test_helpers.rs` - Enhanced with `set_response_sequence()` and `consume_response()` for retry simulation
- [x] `crates/media-control-lib/src/commands/avoid.rs` - 11 new E2E tests + 2 suppress logic tests
- [x] `crates/media-control-lib/src/commands/fullscreen.rs` - 6 new E2E tests

### Key Decisions

- **suppress_ms=0 in test configs**: Eliminates race conditions from the shared suppress file when tests run in parallel
- **Response sequences**: MockHyprland can return different responses for successive calls to the same command, enabling fullscreen retry testing
- **suppress tested directly**: The suppress logic is tested via `should_suppress()` directly rather than through the full avoid flow, avoiding parallel test interference

### Deviations from Plan

- Suppress test changed from E2E (through avoid) to direct unit test of `should_suppress()` to avoid race conditions
- Response sequence feature added to MockHyprland (not in original plan but needed for fullscreen retry tests)

### Dependencies Added

None

### Developer Notes

- The fullscreen exit test uses response sequences: first call returns fullscreen=2, subsequent calls return fullscreen=0 (simulating successful exit)
- Position override test uses TOML parsing to construct PositionOverride (avoids private OnceLock field)
