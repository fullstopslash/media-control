---
id: 016-shim-integration
unit: 001-shim-ipc-updates
intent: 011-mpv-shim-integration
type: simple-construction-bolt
status: complete
stories:
  - 001-new-ipc-commands
  - 002-query-socket
  - 003-keep-simplify
  - 004-status-enrich
created: 2026-03-28T10:00:00.000Z
started: 2026-03-28T10:00:00.000Z
completed: "2026-03-28T20:20:07Z"
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-28T10:00:00.000Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-28T10:00:00.000Z
    artifact: implementation-walkthrough.md
requires_bolts: []
enables_bolts: []
requires_units: []
blocks: false
complexity:
  avg_complexity: 2
  avg_uncertainty: 2
  max_dependencies: 1
  testing_scope: 2
---

# Bolt: 016-shim-integration

## Overview

Integrate media-control with Rust mpv-shim's new capabilities: query socket, new IPC commands, simplified keep routing, enriched status.

## Stories Included

- **001-new-ipc-commands**: switch-store + queue-info (Could)
- **002-query-socket**: Query socket client for play (Should)
- **003-keep-simplify**: Single-socket keep routing (Should)
- **004-status-enrich**: Richer status from query socket (Could)

## Stages

- [ ] **1. Plan**: Implementation plan
- [ ] **2. Implement**: Code changes
- [ ] **3. Test**: Verification

## Notes

**BLOCKED** — waiting for Rust mpv-shim to stabilize before construction.
