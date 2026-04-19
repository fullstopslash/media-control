---
intent: 004-ipc-reliability
phase: inception
status: units-decomposed
updated: 2026-03-19T12:00:00Z
---

# IPC Reliability - Unit Decomposition

## Units Overview

This intent decomposes into 1 unit of work. All 5 FRs target the same function (`send_mpv_script_message()`) and its callers — splitting further would create artificial boundaries.

### Unit 1: ipc-hardening

**Description**: Harden `send_mpv_script_message()` with socket validation, timeouts, response verification, retry logic, and propagate errors with user-visible feedback.

**Stories**:

- 001-socket-validation: stat() before connect, skip non-sockets
- 002-connection-timeout: 500ms timeout on connect+write
- 003-response-verification: Read mpv IPC response with timeout
- 004-stale-socket-retry: Retry once after 100ms on total failure
- 005-error-feedback: Propagate errors to stderr + notify-send + exit code

**Deliverables**:

- Hardened `send_mpv_script_message()` in `crates/media-control-lib/src/commands/mod.rs`
- Error propagation in `mark_watched.rs` callers
- notify-send integration in `main.rs`

**Dependencies**:

- Depends on: None
- Depended by: None

**Estimated Complexity**: M

## Requirement-to-Unit Mapping

- **FR-1**: Socket path validation → `001-ipc-hardening`
- **FR-2**: Connection timeout → `001-ipc-hardening`
- **FR-3**: Error feedback to user → `001-ipc-hardening`
- **FR-4**: Response verification → `001-ipc-hardening`
- **FR-5**: Stale socket retry → `001-ipc-hardening`

## Unit Dependency Graph

```text
[001-ipc-hardening] (standalone)
```

## Execution Order

1. 001-ipc-hardening (single unit, single bolt)
