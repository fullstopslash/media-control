---
id: 002-query-socket
unit: 001-shim-ipc-updates
intent: 011-mpv-shim-integration
status: complete
priority: should
created: 2026-03-28T10:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 002-query-socket

## User Story

**As a** media-control user
**I want** the play command to resolve items via the shim's query socket
**So that** item resolution is sub-millisecond instead of ~100ms HTTP

## Acceptance Criteria

- [ ] play command tries query socket at `/tmp/mpv-shim-query.sock` first
- [ ] Falls back to Jellyfin HTTP API if query socket unavailable
- [ ] Query protocol: JSON request + newline, JSON array response
