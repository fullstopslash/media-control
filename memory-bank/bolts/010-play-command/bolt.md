---
id: 010-play-command
unit: 001-play-command
intent: 005-play-subcommand
type: simple-construction-bolt
status: complete
stories:
  - 001-jellyfin-methods
  - 002-multi-arg-ipc
  - 003-play-config
  - 004-play-command
  - 005-cli-wiring
created: 2026-03-19T18:00:00.000Z
started: 2026-03-19T18:00:00.000Z
completed: "2026-03-20T00:05:07Z"
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-19T18:00:00.000Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-19T18:00:00.000Z
    artifact: implementation-walkthrough.md
requires_bolts: []
enables_bolts: []
requires_units: []
blocks: false
complexity:
  avg_complexity: 1
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

# Bolt: 010-play-command

## Overview

Implement the full `media-control play` subcommand in one pass — Jellyfin API methods, multi-arg IPC helper, config, command module, and CLI wiring.

## Objective

Replace shim-play.sh with a native Rust subcommand that resolves playback targets, sends IPC hints, and initiates playback with resume support.

## Stories Included

- **001-jellyfin-methods**: 3 new API methods (Must)
- **002-multi-arg-ipc**: Multi-arg script-message helper (Must)
- **003-play-config**: PlayConfig struct (Must)
- **004-play-command**: play.rs command module (Must)
- **005-cli-wiring**: Wire into main.rs (Must)

## Bolt Type

**Type**: Simple Construction Bolt
**Definition**: `.specsmd/aidlc/templates/construction/bolt-types/simple-construction-bolt.md`

## Stages

- [-] **1. Plan**: Implementation plan → implementation-plan.md <!-- tw:4a12b4bd-2f9a-4ae3-9921-f2d1b1d0e2ba -->
- [-] **2. Implement**: Code changes → modified source files <!-- tw:ec0f7d98-9704-469c-ad0f-34063b37bdbf -->
- [-] **3. Test**: Verification → test-walkthrough.md <!-- tw:e6d8a922-2799-40c9-b5a2-6947d53ce255 -->

## Dependencies

### Requires
- None (intent 004 IPC hardening already complete)

### Enables
- None

## Success Criteria

- [-] All 5 stories implemented <!-- tw:692d7eab-91af-4abe-8bcb-0fd1ae7c6ca6 -->
- [-] All acceptance criteria met <!-- tw:b625bd26-0cef-4698-882c-dfa17f5e5af4 -->
- [-] `media-control play next-up` works end-to-end <!-- tw:1aa92715-fa18-42ff-9e3d-9f8b94b21083 -->
- [-] `media-control play recent-pinchflat` works end-to-end <!-- tw:94e6b303-1ac5-45a0-90a9-01e7149538cb -->
- [-] Total latency < 200ms <!-- tw:2cb69c52-54ce-4c66-801a-1189122a197c -->
- [-] `cargo clippy` and `cargo test` pass <!-- tw:f574b238-6544-4c82-890d-1b61fa9b7ccc -->

## Notes

~225 lines new code. Key files: jellyfin.rs, commands/mod.rs, config.rs, commands/play.rs, main.rs.
