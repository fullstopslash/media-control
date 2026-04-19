---
id: 001-new-ipc-commands
unit: 001-shim-ipc-updates
intent: 011-mpv-shim-integration
status: complete
priority: could
created: 2026-03-28T10:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 001-new-ipc-commands

## User Story

**As a** media-control user
**I want** switch-store and queue-info commands
**So that** I can switch between Jellyfin/Stash and inspect queue state

## Acceptance Criteria

- [ ] `send_mpv_script_message_with_args("switch-store", &["jellyfin"])` works
- [ ] `queue-info` response is parsed and returned as JSON
- [ ] Commands degrade gracefully on old Python shim (ignored, no error)
