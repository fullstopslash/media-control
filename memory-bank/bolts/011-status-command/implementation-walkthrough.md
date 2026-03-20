---
stage: implement
bolt: 011-status-command
created: 2026-03-19T19:00:00Z
---

## Implementation Walkthrough: status-command

### Summary

Implemented `media-control status [--json]` that queries mpv IPC for playback properties. Added `query_mpv_property()` as a new IPC primitive that returns response data, plus a status command with dual output format.

### Structure Overview

The status command bypasses Hyprland/config entirely — it connects directly to mpv's IPC socket, queries 4 properties, and formats output. Routed in main.rs before config loading for minimal overhead.

### Completed Work

- [x] `crates/media-control-lib/src/commands/mod.rs` - query_mpv_property() function + pub mod status <!-- tw:69c34915-3bb0-413e-ae88-95b56a4fc66b -->
- [x] `crates/media-control-lib/src/commands/status.rs` - New module: dual output format, not-playing detection <!-- tw:f76deead-99d0-46ff-aab0-80947fbdc44a -->
- [x] `crates/media-control/src/main.rs` - Status { json } variant, routed before config loading <!-- tw:f0dc16e5-dfb7-4ac2-968a-2c6dced6b33c -->

### Key Decisions

- **query_mpv_property returns serde_json::Value**: Generic enough for any property type (string, number, bool)
- **Single attempt, no retry**: Status should be fast or fail — 200ms total timeout
- **Routed before config loading**: Status only needs mpv IPC, not Hyprland or config.toml
- **Exit code 1 for not playing**: Makes waybar/scripting integration trivial

### Deviations from Plan

None.

### Dependencies Added

None.
