---
id: 004-status-enrich
unit: 001-shim-ipc-updates
intent: 011-mpv-shim-integration
status: complete
priority: could
created: 2026-03-28T10:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 004-status-enrich

## User Story

**As a** media-control user
**I want** richer status info from the query socket
**So that** waybar/scripts can show current store, queue depth, cache state

## Acceptance Criteria

- [ ] `--json` output includes `store`, `queue_depth`, `cache_fresh` when query socket available
- [ ] Fields absent when query socket unavailable (backward compatible)
