---
intent: 011-mpv-shim-integration
phase: inception
status: units-decomposed
updated: 2026-03-28T10:00:00Z
---

# mpv-shim Integration - Unit Decomposition

## Units Overview

1 unit — all 4 FRs touch the same IPC/command layer.

### Unit 1: shim-ipc-updates

**Description**: Add query socket client, new IPC commands, simplify keep routing, enrich status.

**Requirement Mapping**:
- FR-1 → 001-new-ipc-commands
- FR-2 → 002-query-socket
- FR-3 → 003-keep-simplify
- FR-4 → 004-status-enrich
