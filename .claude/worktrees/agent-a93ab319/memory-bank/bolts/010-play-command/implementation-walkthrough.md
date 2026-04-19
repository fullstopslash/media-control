---
stage: implement
bolt: 010-play-command
created: 2026-03-19T18:00:00Z
---

## Implementation Walkthrough: play-command

### Summary

Implemented `media-control play <target>` subcommand that replaces shim-play.sh. Three Jellyfin API methods, multi-arg IPC helper, config struct, command module with PlayTarget enum, and CLI wiring.

### Structure Overview

The play command follows a 4-step pipeline: resolve item ID → send IPC hint → get resume ticks → find session + PlayNow. All steps reuse existing JellyfinClient and IPC infrastructure.

### Completed Work

- [x] `crates/media-control-lib/src/jellyfin.rs` - 3 new methods + 2 new deserialization structs (ItemDetail, ItemUserData) <!-- tw:820be9cf-8673-4a79-bcbb-0da1b4d34fb9 -->
- [x] `crates/media-control-lib/src/commands/mod.rs` - send_mpv_script_message_with_args helper + pub mod play <!-- tw:1107f425-8c4d-4c13-84fb-cbeab8be7704 -->
- [x] `crates/media-control-lib/src/config.rs` - PlayConfig struct + wired into Config with serde(default) <!-- tw:112c1b98-b4e5-409d-b7a8-30984bcae840 -->
- [x] `crates/media-control-lib/src/commands/play.rs` - New module: PlayTarget enum, play() orchestration <!-- tw:cfd0cd48-cf73-4d53-b2ba-14a90e753af3 -->
- [x] `crates/media-control/src/main.rs` - Play { target } variant + match arm <!-- tw:a430e192-dc81-4173-82a4-4911e8050c25 -->

### Key Decisions

- **Box<dyn Error> return type for play()**: JellyfinClient returns JellyfinError, commands use MediaControlError — no From impl exists. Using Box<dyn Error> avoids coupling the error types since main.rs already handles Box<dyn Error>.
- **String error messages for domain errors**: "No next-up item found", "Shim not connected" etc. are user-facing messages passed as &str errors, not structured error types. Appropriate for a one-shot CLI.
- **IPC hint failure is non-fatal**: Shim may not be running when play is invoked. Log warning and continue to PlayNow.

### Deviations from Plan

None.

### Dependencies Added

None.
