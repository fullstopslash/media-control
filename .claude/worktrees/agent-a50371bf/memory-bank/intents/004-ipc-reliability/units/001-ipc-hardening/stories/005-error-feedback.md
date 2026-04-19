---
id: 005-error-feedback
unit: 001-ipc-hardening
intent: 004-ipc-reliability
status: complete
priority: must
created: 2026-03-19T12:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 005-error-feedback

## User Story

**As a** media-control user
**I want** failed commands to show a visible error
**So that** I know the command didn't go through and can take action (retry, check mpv, etc.)

## Acceptance Criteria

- [-] **Given** send_mpv_script_message returns an error, **When** the CLI handles it, **Then** a brief error is printed to stderr <!-- tw:bc01bfa4-dcad-417a-9a0b-1e437cf87cd0 -->
- [-] **Given** send_mpv_script_message returns an error, **When** the CLI handles it, **Then** the process exits with non-zero code <!-- tw:380f13f8-fbd4-484b-b14f-702013cbde35 -->
- [-] **Given** send_mpv_script_message returns an error, **When** notify-send is available, **Then** a desktop notification is shown with the error <!-- tw:c441dd52-7c39-4242-a0e0-5c6442144913 -->
- [-] **Given** notify-send is not available, **When** an error occurs, **Then** stderr output still works (graceful degradation) <!-- tw:2cf1e1bc-58da-4821-94c1-1ab1b735ba36 -->

## Technical Notes

- Callers in `mark_watched.rs` currently swallow errors — need to propagate `Result`
- `main.rs` should catch errors and: (1) eprintln, (2) notify-send, (3) exit(1)
- notify-send invocation: `notify-send -u critical "media-control" "Failed: <cmd> — <reason>"`
- Use `std::process::Command` for notify-send, don't wait for it (fire and forget)

## Dependencies

### Requires
- 001-socket-validation (validation errors need reporting)
- 002-connection-timeout (timeout errors need reporting)
- 003-response-verification (response errors need reporting)
- 004-stale-socket-retry (final retry failure needs reporting)

### Enables
- None (terminal story)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| notify-send binary missing | Log warning once, degrade to stderr only |
| Very long error message | Truncate to reasonable length for notification |
| Multiple rapid failures | Each gets its own notification |

## Out of Scope

- Notification history or aggregation
- Retry from notification action
