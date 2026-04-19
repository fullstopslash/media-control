---
id: 002-multi-arg-ipc
unit: 001-play-command
intent: 005-play-subcommand
status: complete
priority: must
created: 2026-03-19T18:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 002-multi-arg-ipc

## User Story

**As a** media-control developer
**I want** a `send_mpv_script_message_with_args` helper
**So that** I can send multi-argument script-messages like `set-play-source nextup`

## Acceptance Criteria

- [-] **Given** message "set-play-source" and args ["nextup"], **When** helper is called, **Then** it sends `{"command":["script-message","set-play-source","nextup"]}` <!-- tw:f74f6c38-20c9-44b2-b351-d2ccdefda0d8 -->
- [-] **Given** message "foo" and empty args, **When** helper is called, **Then** it behaves like `send_mpv_script_message("foo")` <!-- tw:87a29b92-8bba-4f1c-ac85-00930f990bcf -->

## Technical Notes

- Build command array: `["script-message", message, ...args]`
- Serialize via `serde_json::json!({"command": parts})`
- Delegate to existing `send_mpv_ipc_command()` for the hardened send path

## Dependencies

### Requires
- Intent 004 IPC hardening (send_mpv_ipc_command)

### Enables
- 004-play-command (sends IPC hint)
