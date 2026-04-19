---
unit: 001-shim-ipc-updates
intent: 011-mpv-shim-integration
phase: inception
status: complete
created: 2026-03-28T10:00:00.000Z
updated: 2026-03-28T10:00:00.000Z
---

# Unit Brief: Shim IPC Updates

## Purpose

Integrate media-control with the Rust mpv-shim's new capabilities while maintaining backward compatibility.

## Scope

### In Scope
- switch-store and queue-info IPC commands
- Query socket client for item resolution
- Simplified keep routing (single socket)
- Enriched status output from query socket

### Out of Scope
- Changes to the shim itself
- Breaking changes to existing IPC protocol

## Story Summary

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-new-ipc-commands | switch-store + queue-info | Could | Planned |
| 002-query-socket | Query socket client for play | Should | Planned |
| 003-keep-simplify | Single-socket keep routing | Should | Planned |
| 004-status-enrich | Richer status from query socket | Could | Planned |

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 016-shim-integration | simple-construction-bolt | all 4 | Full shim integration in one pass |
