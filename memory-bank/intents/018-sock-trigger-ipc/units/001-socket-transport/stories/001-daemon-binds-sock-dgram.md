---
id: 001-daemon-binds-sock-dgram
unit: 001-socket-transport
intent: 018-sock-trigger-ipc
status: complete
priority: must
created: 2026-05-03T15:44:51.000Z
assigned_bolt: 030-socket-transport
implemented: true
---

# Story: 001-daemon-binds-sock-dgram

## User Story

**As a** daemon process starting up
**I want** to bind a `SOCK_DGRAM` UNIX socket at a stable, single-source-of-truth path with TOCTOU-safe creation
**So that** the daemon presents a consistent IPC surface (one socket, one path, one mode) and a malicious or misconfigured pre-existing entry at the path cannot trick the daemon into binding inside an attacker-chosen target

## Acceptance Criteria

- [ ] **Given** a clean `$XDG_RUNTIME_DIR`, **When** the daemon starts, **Then** a `SOCK_DGRAM` socket is bound at `$XDG_RUNTIME_DIR/media-control-daemon.sock` with mode `0o600` and `ss -lx` shows it as a `u_dgr` listener
- [ ] **Given** a pre-existing symlink at the bind path, **When** the daemon starts, **Then** it refuses to bind (no symlink-following), logs an `error`, and exits non-zero
- [ ] **Given** a pre-existing regular file at the bind path, **When** the daemon starts, **Then** it refuses to bind, logs an `error`, and exits non-zero
- [ ] **Given** a pre-existing socket at the bind path owned by a different uid, **When** the daemon starts, **Then** it refuses to bind, logs an `error`, and exits non-zero
- [ ] **Given** a pre-existing socket at the bind path owned by us (e.g. from a previous daemon run), **When** the daemon starts, **Then** it `unlink`s the stale socket and binds successfully
- [ ] **Given** the bind succeeds, **When** any consumer queries the socket path via `media-control-lib::socket_path()`, **Then** the result equals the daemon's bound path (single source of truth)
- [ ] **Given** the lstat-then-bind sequence, **When** read by a security reviewer, **Then** the path validation uses `lstat` (not `stat`) so symlinks at the bind path are detected and rejected without being followed

## Technical Notes

- Add `media-control-lib::socket_path()` (or equivalent in a new `transport` module) returning `runtime_dir().join("media-control-daemon.sock")`. Single constant for the filename.
- Mirror `create_fifo_at` posture in `crates/media-control-daemon/src/main.rs:394`. Likely extract a generic `create_unix_endpoint_at(path, kind)` helper since the lstat-validate-unlink sequence is now used for both the FIFO removal path (story 004) and the new socket — though by end of unit the FIFO version is deleted, so the helper can specialize to socket creation only. Construction-stage decision.
- Use `nix::sys::stat::lstat` + `nix::unistd::unlink` (or libc equivalents) — same crate the existing code uses.
- Bind mode `0o600` per Q4. After bind, explicit `chmod` if the bind doesn't honour umask correctly (test for this on the target system; some `UnixDatagram::bind` implementations apply umask).
- On bind failure: log at `error` level, return error from main → exit non-zero. Mirrors the current `create_fifo` failure severity.
- 4 unit tests targeting the TOCTOU surface, replacing the 4 FIFO tests:
  - `binds_at_fresh_path` (file-type check via `is_socket()`, **not** inode equality — per CLAUDE.md memory and `media-control-t8d` lessons)
  - `rejects_symlink_at_path`
  - `rejects_regular_file_at_path`
  - `rebinds_over_our_own_existing_socket` (replaces the inode-comparison test that was destabilised by tmpfs inode reuse)

## Dependencies

### Requires

- None (first story in the unit)

### Enables

- 002-dgram-listener-replaces-fifo (the listener task receives on the socket bound here)
- 003-cli-kick-subcommand (CLI's `kick()` resolves the socket path via the same lib helper)
- 004-daemon-fifo-cleanup (FIFO cleanup runs after socket bind succeeds — symmetric error recovery)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `$XDG_RUNTIME_DIR` doesn't exist | Daemon refuses to start with a clear error; this case is also broken today |
| Bind path's parent dir is non-writable | Daemon refuses to start with a clear error |
| Pre-existing socket owned by us but daemon-locked by another running daemon | Unlink succeeds (we own it); new bind may fail with EADDRINUSE if the other daemon is still listening; this is a misconfigured-system case — log error and exit |
| `umask` is unusually permissive (e.g. `0o000`) | After bind, explicit `chmod 0o600` ensures mode regardless of umask |
| tmpfs sandbox in nix build (CLAUDE.md memory warning) | Tests assert via `is_socket()` + bind-success, not inode equality |

## Out of Scope

- The listener task that receives on the bound socket — story 002.
- The CLI consumer that sends to the bound socket — story 003.
- Cleanup of the legacy FIFO at the old path — story 004 (runs *after* this story's bind succeeds for symmetric error recovery).
