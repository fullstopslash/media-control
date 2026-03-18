---
stage: implement
bolt: 001-mock-infrastructure
created: 2026-03-18T14:30:00Z
---

## Implementation Walkthrough: mock-infrastructure

### Summary

Built a complete mock Hyprland IPC test infrastructure: a tokio-based mock socket server with configurable responses and command capture, plus test context builders for end-to-end command testing.

### Structure Overview

Single new module `test_helpers.rs` behind `#[cfg(test)]`, plus minor additions to existing modules for test constructors.

### Completed Work

- [x] `crates/media-control-lib/src/test_helpers.rs` - Mock server, command capture, test context builders, JSON helpers, test client/monitor constructors
- [x] `crates/media-control-lib/src/hyprland.rs` - Made `with_socket_path` always available (removed `#[cfg(test)]`), added `Serialize` derive to `Client`, `Monitor`, `Workspace`
- [x] `crates/media-control-lib/src/commands/mod.rs` - Added `CommandContext::for_test()` constructor
- [x] `crates/media-control-lib/src/lib.rs` - Registered `test_helpers` module

### Key Decisions

- **Single module**: All test infrastructure in one file rather than a directory - keeps it simple and discoverable
- **`with_socket_path` always public**: No harm in exposing it, and avoids `cfg(test)` visibility issues across crate boundaries
- **Serialize derives on IPC types**: Needed for JSON roundtrip helpers; no runtime cost since serde is already a dependency
- **Arc<Mutex<_>> for shared state**: Standard pattern for sharing between mock server task and test code
- **Prefix matching for responses**: Allows setting "dispatch" → "ok" once instead of configuring every possible dispatch command

### Deviations from Plan

None

### Dependencies Added

None (uses existing `tempfile`, `tokio`, `serde_json`)

### Developer Notes

- Each mock server connection spawns its own task, so concurrent commands (within batch) work naturally
- The temp dir is moved into the server task to keep the socket alive for the server's lifetime
- `find_response` uses HashMap iteration for prefix matching - fine for test code with small response maps
