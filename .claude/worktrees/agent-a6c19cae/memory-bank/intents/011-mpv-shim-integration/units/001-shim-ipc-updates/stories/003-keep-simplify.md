---
id: 003-keep-simplify
unit: 001-shim-ipc-updates
intent: 011-mpv-shim-integration
status: complete
priority: should
created: 2026-03-28T10:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 003-keep-simplify

## User Story

**As a** media-control developer
**I want** keep to send to a single socket instead of broadcasting
**So that** routing is handled by the shim's plugin system

## Acceptance Criteria

- [ ] keep sends only to `/tmp/mpvctl-jshim` (shim routes internally)
- [ ] Still works with standalone mpv (falls through socket list)
- [ ] KEEP_SOCKETS reduced or made configurable
