---
id: 002-dgram-listener-replaces-fifo
unit: 001-socket-transport
intent: 018-sock-trigger-ipc
status: complete
priority: must
created: 2026-05-03T15:44:51.000Z
assigned_bolt: 030-socket-transport
implemented: true
---

# Story: 002-dgram-listener-replaces-fifo

## User Story

**As a** daemon process holding a bound trigger socket
**I want** an async listener task that receives datagrams, treats 0-byte payloads as canonical kicks, ignores non-empty payloads with a debug log, and recovers from transient errors with bounded backoff
**So that** the avoider re-evaluation channel is fed exactly as the FIFO listener fed it today, and the wire format is locked (FR-9) such that future protocol versions can be added without breaking the canonical kick

## Acceptance Criteria

- [ ] **Given** the daemon is running with a bound trigger socket, **When** an empty datagram arrives, **Then** exactly one `Processing trigger` log line is emitted and exactly one avoid pass occurs within ~50ms
- [ ] **Given** the daemon is running, **When** 100 empty datagrams arrive in a tight loop, **Then** the avoider performs ≤ 101 evaluations (channel coalescing via `try_send` is preserved)
- [ ] **Given** the daemon is running, **When** a 1-byte datagram with payload `[0x01]` arrives, **Then** the daemon emits a single `debug!` log line ("Ignoring v1 datagram (unsupported in this release)" or equivalent) and triggers no avoid pass
- [ ] **Given** the daemon is running, **When** a 1-byte datagram with payload `[0xFF]` arrives, **Then** the daemon emits a single `debug!` log line and triggers no avoid pass
- [ ] **Given** the daemon is running, **When** a multi-byte datagram arrives starting with byte `0x01`, **Then** the daemon emits a single `debug!` log line and triggers no avoid pass (forward-compat with v1 envelope)
- [ ] **Given** `recv_from` returns an error other than `WouldBlock`, **When** the listener observes it, **Then** it logs at `warn`, sleeps `SOCKET_ERROR_BACKOFF` (~100ms), and continues without crashing the daemon
- [ ] **Given** a sustained `recv_from` error storm (mock fault injection), **When** the daemon runs for 10 seconds in that state, **Then** CPU usage on the listener thread stays ≤ 10%
- [ ] **Given** the daemon shuts down, **When** the `dgram_listener_handle` (formerly `fifo_listener_handle`) is dropped, **Then** the AbortOnDrop guard cancels the spawned task cleanly

## Technical Notes

- Replace `fifo_listener` (`crates/media-control-daemon/src/main.rs:475`) with `dgram_listener`. Same async-loop shape; swap `tokio::fs::File::open(fifo).await + read_line` for `socket.recv_from(&mut buf).await`.
- The buffer should be sized for the expected max — for now 16 bytes is plenty (we ignore non-empty datagrams; we just need to know the length to classify). Discard truncated reads as if they were the full datagram (same length classification).
- Coalesce into the existing `mpsc::Sender<()>` via `try_send`; on `Full(_)` drop and continue (matches current `fifo_listener` behavior — backpressure-free).
- Rename `fifo_listener_handle` → `dgram_listener_handle` in `run_event_loop`. Keep AbortOnDrop wrapper unchanged (Q3).
- Define `SOCKET_ERROR_BACKOFF` constant (~100ms) co-located with the existing `FIFO_ERROR_BACKOFF` (which is being deleted in story 004, so the new constant outlives it).
- For the version-byte log line: include the version byte value in the message for debuggability, e.g. `debug!("Ignoring v{:#04x} datagram ({} bytes, unsupported in this release)", buf[0], n)`.
- Update the trigger channel's `tokio::select!` arm in `run_event_loop` to read `dgram_rx` (renamed from `fifo_rx`).
- Tests: at least one integration test that bounces a real datagram through to the channel via mock or scaffolded test harness. The unit test for the recv-error backoff can use mock `recv_from` injection if the existing test infrastructure supports it; otherwise an integration-style test with a deliberately-broken socket suffices.

## Dependencies

### Requires

- 001-daemon-binds-sock-dgram (the listener receives on the socket bound by story 001)

### Enables

- 003-cli-kick-subcommand (the CLI sends datagrams that this listener receives — round-trip test in story 003 depends on this)
- 004-daemon-fifo-cleanup (story 004 deletes the FIFO listener that this story replaces, so the deletion happens in the same release as the replacement)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| Datagram arrives during shutdown (after AbortOnDrop fires but before task observes cancellation) | Lost datagram acceptable (no shutdown-time delivery guarantee); next daemon process resumes with empty channel |
| Buffer too small for unusually large datagram | Truncated; classified by length received (still ≥ 1 → ignored); no panic |
| `recv_from` returns Ok with Sender disconnected (avoider task gone) | Listener exits cleanly via channel-closed branch; matches current `fifo_listener` behavior |
| Spurious `WouldBlock` returns (shouldn't happen with default tokio recv but the existing code defends) | Loop continues without backoff (matches current behavior) |
| Non-empty datagram with byte 0 = 0x00 (would be a length-1 payload of just version 0) | Treated as version-0 reserved; `debug!` log, ignore. Not the same as a length-0 canonical kick. |

## Out of Scope

- Decoding any v1 envelope payload — future intent. This story only locks the recv side of the wire format.
- Adding a CLI flag to send non-empty datagrams — story 003 explicitly does NOT expose payload-shaping flags (FR-9 enforcement).
- Changing the avoider's per-event triggering logic on Hyprland socket2 events — explicit non-goal of the intent.
