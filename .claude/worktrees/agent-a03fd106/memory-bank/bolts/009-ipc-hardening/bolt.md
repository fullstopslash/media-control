---
id: 009-ipc-hardening
unit: 001-ipc-hardening
intent: 004-ipc-reliability
type: simple-construction-bolt
status: complete
stories:
  - 001-socket-validation
  - 002-connection-timeout
  - 003-response-verification
  - 004-stale-socket-retry
  - 005-error-feedback
created: 2026-03-19T12:00:00.000Z
started: 2026-03-19T12:00:00.000Z
completed: "2026-03-19T07:22:35Z"
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-19T12:00:00.000Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-19T12:00:00.000Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-03-19T12:00:00.000Z
    artifact: test-walkthrough.md
requires_bolts: []
enables_bolts: []
requires_units: []
blocks: false
complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

# Bolt: 009-ipc-hardening

## Overview

Harden `send_mpv_script_message()` with socket validation, timeouts, response verification, retry logic, and user-visible error feedback. Single bolt covering all 5 stories since they modify the same function and its callers.

## Objective

Transform unreliable, silent-failure IPC into robust command delivery with sub-second latency, automatic retry, and clear error feedback.

## Stories Included

- **001-socket-validation**: stat() before connect, skip non-sockets (Must)
- **002-connection-timeout**: 500ms timeout on connect+write (Must)
- **003-response-verification**: Read mpv IPC response with 200ms timeout (Should)
- **004-stale-socket-retry**: 100ms wait + retry once on total failure (Should)
- **005-error-feedback**: Propagate errors to stderr + notify-send + exit code (Must)

## Bolt Type

**Type**: Simple Construction Bolt
**Definition**: `.specsmd/aidlc/templates/construction/bolt-types/simple-construction-bolt.md`

## Stages

- [-] **1. Plan**: Implementation plan → implementation-plan.md <!-- tw:96894118-0b7f-4f1a-bfbd-63364ff61ddd -->
- [-] **2. Implement**: Code changes → modified source files <!-- tw:e9b724b8-6533-44b5-afde-ef749de80611 -->
- [-] **3. Test**: Verification → test-walkthrough.md <!-- tw:0c0a2aa1-ca0b-40b7-9a3b-e2eadde93e25 -->

## Dependencies

### Requires
- None (standalone bolt)

### Enables
- None

## Success Criteria

- [-] All 5 stories implemented <!-- tw:a3e1a33e-c985-4f92-90d3-55d390b722ee -->
- [-] All acceptance criteria met <!-- tw:03ed8527-c207-48a5-afd6-edf2e50fcdee -->
- [-] No silent IPC failures <!-- tw:12a5d095-7f74-4e8f-ab5b-ad1195c7374e -->
- [-] Happy path < 200ms, retry path < 800ms <!-- tw:067863d6-b17b-4a6b-831e-8c8ce13fa426 -->
- [-] Desktop notification on error <!-- tw:673fff98-8b5a-45ae-aa52-513ce41a7570 -->

## Notes

Key files to modify:
- `crates/media-control-lib/src/commands/mod.rs` — send_mpv_script_message()
- `crates/media-control-lib/src/commands/mark_watched.rs` — error propagation
- `crates/media-control/src/main.rs` — error handling + notify-send
