---
intent: 004-ipc-reliability
phase: inception
status: complete
created: 2026-03-19T12:00:00.000Z
updated: 2026-03-19T12:00:00.000Z
---

# Requirements: IPC Reliability

## Intent Overview

Fix unreliable and slow IPC command delivery from media-control to jellyfin-mpv-shim. Commands like mark-watched-and-next, skip-next, skip-prev take 3+ seconds or silently fail. The root cause is that `send_mpv_script_message()` has no socket validation, no timeouts, no error feedback, and no response verification.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Commands respond sub-second | Keypress-to-action < 1s p95 | Must |
| Failed commands are visible | User sees error notification on failure | Must |
| Stale sockets don't block | Socket validation before connect | Must |
| mpv respawn is tolerable | Retry covers respawn window | Should |

---

## Functional Requirements

### FR-1: Socket Path Validation
- **Description**: `stat()` socket path before connecting. If not a Unix socket (S_ISSOCK), skip to next path and log a warning.
- **Acceptance Criteria**: Connecting to a regular file (left by socat) is skipped instantly; next socket path is tried.
- **Priority**: Must

### FR-2: Connection Timeout
- **Description**: Wrap `connect()` + `write()` in a 500ms timeout. On timeout, try next socket path. Return error if all paths fail.
- **Acceptance Criteria**: A dead/unresponsive socket returns error within 500ms instead of hanging.
- **Priority**: Must

### FR-3: Error Feedback to User
- **Description**: Propagate `send_mpv_script_message()` errors to `main()`. Print brief error to stderr. Exit non-zero. Send desktop notification via `notify-send`.
- **Acceptance Criteria**: Failed command prints error to stderr, exits non-zero, and shows desktop notification.
- **Priority**: Must

### FR-4: Response Verification
- **Description**: After sending JSON command, read mpv IPC response with 200ms timeout. Log warnings on error responses.
- **Acceptance Criteria**: Successful command reads `{"error":"success"}` response. Error response is logged.
- **Priority**: Should

### FR-5: Stale Socket Retry
- **Description**: If connect fails on all paths, wait 100ms and retry once. Covers the mpv respawn window (~100-500ms).
- **Acceptance Criteria**: A command sent during mpv respawn succeeds on retry within the respawn window.
- **Priority**: Should

---

## Non-Functional Requirements

### Performance
| Requirement | Metric | Target |
|-------------|--------|--------|
| Command latency (happy path) | Keypress to mpv action | < 200ms |
| Command latency (retry path) | Keypress to mpv action with retry | < 800ms |
| Timeout ceiling | Max wait before error | 500ms per socket |

### Reliability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Command delivery rate | Successful commands / total | > 95% during normal operation |
| Error visibility | Failed commands with user feedback | 100% |

---

## Constraints

### Technical Constraints

**Project-wide standards**: Rust workspace with tokio async runtime.

**Intent-specific constraints**:
- Must use tokio for async socket operations (existing runtime)
- Socket paths: `$MPV_IPC_SOCKET` → `/tmp/mpvctl-jshim` → `/tmp/mpvctl0`
- mpv IPC protocol: JSON commands terminated by newline, JSON responses
- Desktop notifications via `notify-send` (already available on user's Arch/Hyprland setup)

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| mpv respawn takes 100-500ms | Retry window too short | Make retry delay configurable |
| `/tmp/mpvctl-jshim` is recreated on mpv respawn | Socket path changes | Fall through to other paths |
| `notify-send` is available | No desktop notification | Degrade to stderr only |

---

## Open Questions

| Question | Owner | Due Date | Resolution |
|----------|-------|----------|------------|
| Should retry count be configurable? | rain | — | Pending |
| Should timeouts be configurable? | rain | — | Pending |
