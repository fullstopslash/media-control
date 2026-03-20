---
unit: 001-ipc-hardening
intent: 004-ipc-reliability
phase: inception
status: complete
created: 2026-03-19T12:00:00.000Z
updated: 2026-03-19T12:00:00.000Z
---

# Unit Brief: IPC Hardening

## Purpose

Harden the IPC command delivery path from media-control to mpv so that commands are delivered reliably, failures are detected quickly, and the user always gets feedback.

## Scope

### In Scope
- Socket path validation (stat before connect)
- Connection and write timeouts
- Reading and verifying mpv IPC responses
- Retry logic for mpv respawn window
- Error propagation to callers
- Desktop notification on failure

### Out of Scope
- Changes to jellyfin-mpv-shim's IPC handler
- mpv respawn logic (owned by shim)
- New commands or protocol changes

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Socket path validation (stat, skip non-sockets) | Must |
| FR-2 | Connection timeout (500ms per socket) | Must |
| FR-3 | Error feedback (stderr + notify-send + exit code) | Must |
| FR-4 | Response verification (read mpv IPC JSON response) | Should |
| FR-5 | Stale socket retry (100ms wait, retry once) | Should |

---

## Domain Concepts

### Key Entities
| Entity | Description | Attributes |
|--------|-------------|------------|
| SocketPath | Candidate IPC socket path | path, is_valid_socket, priority |
| IpcCommand | JSON command to send to mpv | command array, serialized JSON |
| IpcResponse | JSON response from mpv | error field, data |

### Key Operations
| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| validate_socket | stat() path, check S_ISSOCK | path | bool + warning |
| connect_with_timeout | tokio timeout around UnixStream::connect | path, timeout | Result<stream> |
| send_and_receive | write command, read response | stream, command | Result<response> |
| send_with_retry | try all paths, retry once on total failure | command | Result<response> |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 5 |
| Must Have | 3 |
| Should Have | 2 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-socket-validation | Socket path validation | Must | Planned |
| 002-connection-timeout | Connection timeout | Must | Planned |
| 003-response-verification | Response verification | Should | Planned |
| 004-stale-socket-retry | Stale socket retry | Should | Planned |
| 005-error-feedback | Error feedback to user | Must | Planned |

---

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| None | Standalone unit |

### Depended By
| Unit | Reason |
|------|--------|
| None | |

### External Dependencies
| System | Purpose | Risk |
|--------|---------|------|
| mpv IPC socket | Command delivery target | High — mpv dies frequently |
| notify-send | Error notification | Low — degrades to stderr |

---

## Technical Context

### Suggested Technology
- tokio::net::UnixStream for async socket operations
- tokio::time::timeout for timeouts
- tokio::fs::metadata / std::os::unix::fs::FileTypeExt for socket validation
- std::process::Command for notify-send

### Integration Points
| Integration | Type | Protocol |
|-------------|------|----------|
| mpv IPC | Unix socket | JSON newline-delimited |
| notify-send | subprocess | CLI invocation |

---

## Constraints

- Must use existing tokio runtime
- Socket path discovery order: $MPV_IPC_SOCKET → /tmp/mpvctl-jshim → /tmp/mpvctl0
- mpv IPC protocol: JSON command + newline → JSON response + newline

---

## Success Criteria

### Functional
- [ ] Non-socket paths are skipped with warning <!-- tw:7e02af09-6ed8-42d7-8c18-63c57d019947 -->
- [ ] Dead sockets timeout within 500ms <!-- tw:ebda85d1-47c3-4653-aeea-ac4b384ccaa5 -->
- [ ] mpv IPC response is read and validated <!-- tw:f9dc0be1-ec45-4bf8-82f2-4f1e475760e1 -->
- [ ] Commands during mpv respawn succeed on retry <!-- tw:5ee360b5-a371-478f-b547-fed6c33340e9 -->
- [ ] All failures produce stderr output + desktop notification <!-- tw:0f7dfc73-8790-452e-9596-706677aa1dca -->

### Non-Functional
- [ ] Happy path latency < 200ms <!-- tw:5d609b1c-6878-43bf-b728-e01bed27b87b -->
- [ ] Worst case (retry) latency < 800ms <!-- tw:eb30c7b7-7912-408d-865d-648019b47aa8 -->
- [ ] No silent failures <!-- tw:d88a2500-1b35-4bf4-b53f-1da808b41139 -->

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 009-ipc-hardening | simple-construction-bolt | all 5 | Full IPC hardening in one pass |

---

## Notes

All 5 stories modify the same function and its callers. A single bolt is appropriate — the changes are tightly coupled and best implemented together.
