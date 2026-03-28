---
id: 001-socket-validation
unit: 001-ipc-hardening
intent: 004-ipc-reliability
status: complete
priority: must
created: 2026-03-19T12:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 001-socket-validation

## User Story

**As a** media-control user
**I want** socket paths validated before connection attempts
**So that** stale regular files (left by socat or debugging) don't cause hangs or slow failures

## Acceptance Criteria

- [-] **Given** a socket path that is a regular file, **When** send_mpv_script_message tries it, **Then** it is skipped immediately and a warning is logged to stderr <!-- tw:625c66d7-e6a9-4d05-8f91-a6726390aa67 -->
- [-] **Given** a socket path that is a Unix socket, **When** send_mpv_script_message tries it, **Then** it proceeds to connect <!-- tw:ddbeae64-31be-492b-843b-af2d45c8dcb3 -->
- [-] **Given** a socket path that doesn't exist, **When** send_mpv_script_message tries it, **Then** it is skipped and the next path is tried <!-- tw:046ed077-0105-4db9-aa21-3079df118d69 -->
- [-] **Given** all socket paths are invalid, **When** send_mpv_script_message runs, **Then** it returns an error (not a hang) <!-- tw:955fac20-fce7-497b-8395-db4c55e4dc5a -->

## Technical Notes

- Use `std::fs::metadata()` + `std::os::unix::fs::FileTypeExt::is_socket()` to check
- This check happens before `UnixStream::connect()` — it's a pre-filter
- Log warnings via `eprintln!` for skipped paths

## Dependencies

### Requires
- None (first story, modifies existing function)

### Enables
- 002-connection-timeout (validated paths are passed to connect)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| Path is a symlink to a socket | Follow symlink, validate target |
| Path is a directory | Skip with warning |
| Permission denied on stat | Skip with warning, try next path |

## Out of Scope

- Changing the socket path discovery order
- Creating missing sockets
