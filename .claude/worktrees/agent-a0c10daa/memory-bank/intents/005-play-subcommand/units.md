---
intent: 005-play-subcommand
phase: inception
status: units-decomposed
updated: 2026-03-19T18:00:00Z
---

# Play Subcommand - Unit Decomposition

## Units Overview

This intent decomposes into 1 unit. All 8 FRs are tightly coupled — they form a single command flow (resolve → hint → resume → play) modifying 5 files in the same crate.

### Unit 1: play-command

**Description**: Implement the `media-control play` subcommand with target resolution, IPC hint, resume position, and PlayNow orchestration.

**Stories**:

- 001-jellyfin-methods: Add 3 new Jellyfin API methods
- 002-multi-arg-ipc: Add send_mpv_script_message_with_args helper
- 003-play-config: Add PlayConfig to config.rs
- 004-play-command: Create commands/play.rs with PlayTarget + orchestration
- 005-cli-wiring: Wire Play variant into main.rs Commands enum

**Deliverables**:

- 3 new methods in `jellyfin.rs`
- `send_mpv_script_message_with_args` in `commands/mod.rs`
- `PlayConfig` in `config.rs`
- `commands/play.rs` module
- `Play` CLI variant in `main.rs`

**Dependencies**:

- Depends on: Intent 004 IPC hardening (send_mpv_ipc_command)
- Depended by: None

**Estimated Complexity**: S

## Requirement-to-Unit Mapping

- **FR-1**: next-up resolution → `001-play-command`
- **FR-2**: recent-pinchflat resolution → `001-play-command`
- **FR-3**: direct item-id → `001-play-command`
- **FR-4**: IPC play-source hint → `001-play-command`
- **FR-5**: resume position → `001-play-command`
- **FR-6**: session + PlayNow → `001-play-command`
- **FR-7**: config → `001-play-command`
- **FR-8**: error reporting → `001-play-command`

## Unit Dependency Graph

```text
[001-play-command] (standalone, depends on 004-ipc-reliability being complete)
```

## Execution Order

1. 001-play-command (single unit, single bolt)
