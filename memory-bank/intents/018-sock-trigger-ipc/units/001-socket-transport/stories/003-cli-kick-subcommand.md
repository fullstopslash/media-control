---
id: 003-cli-kick-subcommand
unit: 001-socket-transport
intent: 018-sock-trigger-ipc
status: complete
priority: must
created: 2026-05-03T15:44:51.000Z
assigned_bolt: 030-socket-transport
implemented: true
---

# Story: 003-cli-kick-subcommand

## User Story

**As a** Hyprland keybind shell or ad-hoc script
**I want** a `media-control kick` subcommand that sends a 0-byte datagram to the daemon and exits within 100ms regardless of daemon state
**So that** the keybind shell never hangs (no FIFO writer-blocks-on-no-reader hazard) and daemon-down scenarios are silent (exit 0, no dunst spam) while genuine errors still surface (exit 1, stderr message)

## Acceptance Criteria

- [ ] **Given** the daemon is running, **When** I run `media-control kick && echo ok`, **Then** stdout shows `ok` and the daemon journal shows a `Processing trigger` line within ~50ms
- [ ] **Given** the daemon is stopped (`systemctl --user stop media-control-daemon` or `pkill -KILL`), **When** I run `media-control kick && echo ok`, **Then** stdout shows `ok` (exit 0), stderr is empty, and wall time is < 100ms
- [ ] **Given** the daemon socket file does not exist (e.g. `$XDG_RUNTIME_DIR/media-control-daemon.sock` deleted), **When** I run `media-control kick`, **Then** exit code is 0, stderr is empty, wall time < 100ms
- [ ] **Given** the daemon socket exists but is unwritable (e.g. `chmod 000`), **When** I run `media-control kick`, **Then** exit code is 1, stderr contains a clear "permission denied"-class message identifying the path
- [ ] **Given** I run `media-control kick --reason togglesplit`, **When** the CLI parses arguments, **Then** it exits non-zero with an "unrecognized option" / "unexpected argument" message (FR-9 enforcement — no payload-shaping flags exposed in this release)
- [ ] **Given** 1000 invocations of `media-control kick` with the daemon down, **When** I measure wall time, **Then** p99 < 100ms (FR-5)
- [ ] **Given** any error path (permission denied, path resolution failure), **When** the CLI exits with code 1, **Then** stderr message includes the socket path so script callers can debug
- [ ] **Given** both daemon and CLI resolve the socket path via `media-control-lib::socket_path()`, **When** the path is changed in one place, **Then** there is no second place to update (single source of truth verified by code search)

## Technical Notes

- Add `Kick` variant to `Commands` enum in `crates/media-control/src/main.rs`. Route to a new lib helper.
- Add `kick()` in `media-control-lib` (likely in the new `transport` module alongside `socket_path()` from story 001). Async-vs-sync is a design-stage call (open question in requirements.md):
  - Async-symmetric with the rest of `media-control-lib` is consistent.
  - Sync removes the tokio runtime cost from the CLI process startup. The CLI doesn't otherwise need tokio for the `kick` path.
  - Recommend sync; the CLI's tokio runtime spin-up is non-trivial overhead vs. the < 100ms target.
- Implementation core (sync version):
  ```
  let path = socket_path();
  let sock = UnixDatagram::unbound()?;  // std::os::unix::net
  match sock.send_to(&[], &path) {
      Ok(_) => exit(0),
      Err(e) if e.kind() == ErrorKind::ConnectionRefused
             || e.kind() == ErrorKind::NotFound => exit(0),  // FR-4 silent
      Err(e) => { eprintln!("media-control kick: {}: {}", path.display(), e); exit(1); }
  }
  ```
  (Async version analogous via `tokio::net::UnixDatagram`.)
- `UnixDatagram::send_to` on a connectionless socket never blocks on a missing reader — the kernel either delivers, drops, or returns ECONNREFUSED/ENOENT. This is the FR-5 mechanism.
- Error classification:
  - `ConnectionRefused` (ECONNREFUSED) → daemon not accepting → exit 0 silent
  - `NotFound` (ENOENT) → socket file doesn't exist → exit 0 silent
  - `PermissionDenied`, anything else → exit 1 with stderr message
- The argument parser (likely `clap` per current CLI patterns) MUST reject unknown flags by default. Confirm `Kick` variant has no flags defined; verify via test that `media-control kick --reason foo` exits non-zero.
- Integration test ideas:
  - Round-trip: spawn a test daemon, run `media-control kick`, observe channel signal (re-uses test scaffolding from existing daemon tests).
  - Daemon-down silent: bind no socket, run `media-control kick`, assert exit 0 + empty stderr + wall time < 100ms.
  - Socket-unwritable: bind socket, `chmod 000`, run `media-control kick`, assert exit 1 + stderr non-empty.
  - p99 timing: 1000 invocations, daemon down, measure wall time; assert p99 < 100ms.

## Dependencies

### Requires

- 001-daemon-binds-sock-dgram (uses the lib `socket_path()` helper added there)
- 002-dgram-listener-replaces-fifo (round-trip test depends on the listener receiving the kick)

### Enables

- Unit 2 story 001 (Hyprland keybind migration) — keybinds invoke `media-control kick`

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `$XDG_RUNTIME_DIR` unset (degenerate env) | `socket_path()` fails clean; CLI exits 1 with stderr message identifying the env-var issue |
| Socket path resolution succeeds but `send_to` returns ECONNREFUSED on a stale socket file | Treated as silent exit 0 (matches the daemon-down case from the keybind's perspective) |
| Daemon socket exists but daemon process is wedged (recv loop frozen) | `send_to` succeeds (datagram queued in kernel buffer); CLI exits 0; daemon's wedged-ness is a separate concern (intent 017's daemon-stop-hang and possibly a new intent) |
| `media-control kick` invoked under setuid context | Permissions applied based on EUID; if EUID lacks write to the socket, exits 1; this case isn't expected in keybind context |
| Concurrent kicks from multiple keybind shells | All deliver; the daemon's `try_send` coalesces (story 002 acceptance) |

## Out of Scope

- Adding `--reason` or any payload-shaping flag — explicitly forbidden by FR-9 enforcement; reserved for a future telemetry intent.
- Synchronous notification that the daemon actually processed the kick — fire-and-forget by design.
- Async-vs-sync implementation choice — design-stage decision recorded in inception-log Open Questions (deferred to construction).
