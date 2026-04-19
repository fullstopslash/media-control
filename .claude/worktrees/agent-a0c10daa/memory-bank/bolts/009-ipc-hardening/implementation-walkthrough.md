---
stage: implement
bolt: 009-ipc-hardening
created: 2026-03-19T12:00:00Z
---

## Implementation Walkthrough: ipc-hardening

### Summary

Rewrote `send_mpv_script_message()` into a hardened `send_mpv_ipc_command()` with socket validation, timeouts, response verification, and retry logic. Consolidated duplicate IPC code from chapter.rs. Added structured `MpvIpc` error type and desktop notification on failure.

### Structure Overview

The core change is extracting a shared `send_mpv_ipc_command()` that both script-messages and chapter commands use, replacing two separate unhardened implementations with one robust one. Error handling flows from the library through main.rs, which now catches all errors and shows desktop notifications.

### Completed Work

- [x] `crates/media-control-lib/src/error.rs` - Added `MpvIpcErrorKind` enum and `MpvIpc` variant to `MediaControlError` <!-- tw:6b91e656-7019-46e7-8b86-dea2ed07816f -->
- [x] `crates/media-control-lib/src/commands/mod.rs` - Rewrote `send_mpv_script_message()`, added `send_mpv_ipc_command()` with all 5 hardening features <!-- tw:661eec9e-56e8-42a0-825c-1a3364b9751d -->
- [x] `crates/media-control-lib/src/commands/chapter.rs` - Removed duplicate `send_mpv_command()`, now uses shared `send_mpv_ipc_command()` <!-- tw:2307e36c-ffba-4f6a-8bbb-b2dc24ea5ca6 -->
- [x] `crates/media-control-lib/src/commands/mark_watched.rs` - Replaced `let _ =` with `?` to propagate errors <!-- tw:098d7fad-32b9-40f9-bf93-03993f8e15e2 -->
- [x] `crates/media-control/src/main.rs` - Restructured to capture errors, added notify-send integration <!-- tw:d545103d-a2ba-4de8-9838-5b50b1c3fd51 -->

### Key Decisions

- **Consolidated two IPC functions**: Both `send_mpv_script_message` (mod.rs) and `send_mpv_command` (chapter.rs) had identical unhardened logic. Merged into one `send_mpv_ipc_command`.
- **Response read failure is warn-only**: After a successful write, failing to read a response doesn't mean the command failed. Some mpv builds may not respond to script-message.
- **Structured error type over generic Io**: `MpvIpcErrorKind` gives meaningful error messages ("no mpv IPC socket found" vs "I/O error") for desktop notifications.
- **main() returns () not Result**: Restructured to catch errors in `run()` and handle them uniformly with eprintln + notify-send + exit(1).

### Deviations from Plan

- Also refactored chapter.rs to eliminate duplicate IPC code (not in original plan but natural consolidation).

### Dependencies Added

None — all features use existing stdlib and tokio APIs.

### Developer Notes

- `send_mpv_ipc_command` is `pub` so chapter.rs can use it directly for non-script-message commands.
- The retry loop uses `sockets.iter().copied().flatten()` instead of `sockets.into_iter().flatten()` to allow iterating the same array twice.
