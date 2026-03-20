---
id: 003-response-verification
unit: 001-ipc-hardening
intent: 004-ipc-reliability
status: complete
priority: should
created: 2026-03-19T12:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 003-response-verification

## User Story

**As a** media-control user
**I want** mpv IPC responses to be read and verified
**So that** I know whether the command was actually received and processed by mpv

## Acceptance Criteria

- [ ] **Given** a successful write, **When** mpv responds with `{"error":"success"}`, **Then** the command is considered successful <!-- tw:df5d4de5-858f-438d-88b5-406fd1ffaa4c -->
- [ ] **Given** a successful write, **When** mpv responds with an error, **Then** the error is logged and returned <!-- tw:eec5d7a0-b54a-49e7-b318-f70824c72b38 -->
- [ ] **Given** a successful write, **When** no response arrives within 200ms, **Then** the command is considered successful with a warning (fire-and-forget fallback) <!-- tw:b4699e89-b842-4f79-b3ae-1775961b7b84 -->

## Technical Notes

- mpv IPC returns JSON responses terminated by newline
- Response format: `{"error":"success","data":null}` on success
- Read with `tokio::time::timeout(Duration::from_millis(200), read_line)`
- A missing response is a warning, not a hard error — some mpv builds may not respond to script-message

## Dependencies

### Requires
- 002-connection-timeout (needs successful connect+write)

### Enables
- 005-error-feedback (response errors feed into error reporting)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| mpv sends partial response | Timeout after 200ms, warn |
| mpv sends multiple lines | Read first line only |
| Response is not valid JSON | Log warning, treat as success |

## Out of Scope

- Parsing mpv response data fields
- Acting on specific error types from mpv
