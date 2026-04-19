---
id: 002-connection-timeout
unit: 001-ipc-hardening
intent: 004-ipc-reliability
status: complete
priority: must
created: 2026-03-19T12:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 002-connection-timeout

## User Story

**As a** media-control user
**I want** socket operations to timeout quickly
**So that** a dead or unresponsive mpv doesn't cause the command to hang indefinitely

## Acceptance Criteria

- [-] **Given** a valid socket path where mpv is dead, **When** connect is attempted, **Then** it times out within 500ms and tries the next path <!-- tw:eed61627-6f88-4064-bc2f-88390fd66c37 -->
- [-] **Given** a valid socket where mpv is alive, **When** connect + write succeeds, **Then** it completes well under 500ms <!-- tw:d04bc8fb-1df7-4a6c-8702-24ce15072885 -->
- [-] **Given** all socket paths timeout, **When** send_mpv_script_message finishes, **Then** it returns a timeout error <!-- tw:d5c24033-472a-4617-93ab-e04f0ea032bf -->

## Technical Notes

- Wrap `UnixStream::connect()` + `stream.write_all()` in `tokio::time::timeout(Duration::from_millis(500), ...)`
- The 500ms timeout is per-socket-path, not total
- On timeout, log which path timed out and try next

## Dependencies

### Requires
- 001-socket-validation (only validated paths reach connect)

### Enables
- 003-response-verification (after successful connect+write, read response)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| Connect succeeds but write hangs | Timeout covers both connect and write |
| Socket accepts connect but mpv IPC is broken | Timeout on write or response read catches this |

## Out of Scope

- Making timeout values configurable (hardcode 500ms for now)
