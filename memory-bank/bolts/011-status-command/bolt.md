---
id: 011-status-command
unit: 001-status-command
intent: 006-status-subcommand
type: simple-construction-bolt
status: complete
stories:
  - 001-query-mpv-property
  - 002-status-command
  - 003-cli-wiring
created: 2026-03-19T19:00:00.000Z
started: 2026-03-19T19:00:00.000Z
completed: "2026-03-20T00:22:52Z"
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-19T19:00:00.000Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-19T19:00:00.000Z
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

# Bolt: 011-status-command

## Overview

Implement `media-control status` subcommand — query mpv properties, format human-readable and JSON output.

## Objective

Expose mpv playback state for status bar integration and scripting.

## Stories Included

- **001-query-mpv-property**: query_mpv_property() function (Must)
- **002-status-command**: status.rs command module (Must)
- **003-cli-wiring**: Wire Status into main.rs with --json flag (Must)

## Bolt Type

**Type**: Simple Construction Bolt

## Stages

- [-] **1. Plan**: Implementation plan → implementation-plan.md <!-- tw:25229798-e4b8-49bb-a5c7-f9d8f2aa9f17 -->
- [-] **2. Implement**: Code changes → modified source files <!-- tw:2f9350c9-2768-4ab7-a5cb-c92414ac44e9 -->
- [-] **3. Test**: Verification → test-walkthrough.md <!-- tw:c9a80424-753c-44aa-9430-2f66c4348d5f -->

## Dependencies

### Requires
- None (IPC hardening already complete)

### Enables
- None

## Success Criteria

- [-] All 3 stories implemented <!-- tw:780459e4-4b6e-4631-927d-2bfca246b953 -->
- [-] `media-control status` shows playback state <!-- tw:c7b97cf6-9ca5-4383-84d4-f3413035cbf8 -->
- [-] `media-control status --json` emits valid JSON <!-- tw:d23a0ebc-1246-44d9-95ad-88d0668e6fb6 -->
- [-] Exit 0 when playing, exit 1 when not <!-- tw:31177b68-6865-4d63-9834-b8581e57d33d -->
- [-] `cargo clippy` and `cargo test` pass <!-- tw:73732ed5-91d8-47ea-be9f-51149b42a06e -->
