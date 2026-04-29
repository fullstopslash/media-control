---
id: 001-probe-instance
unit: 001-his-resolve-with-probe
intent: 017-daemon-his-autodetect
status: ready
priority: must
created: 2026-04-29T08:10:00Z
assigned_bolt: 028-his-resolve-with-probe
implemented: false
---

# Story: 001-probe-instance

## User Story

**As a** developer adding HIS-liveness awareness to `media-control-lib::hyprland`
**I want** a `probe_instance(his) -> Liveness` function that can classify a Hyprland instance as live-with-clients, live-empty, or dead within a bounded timeout
**So that** `resolve_live_his()` (next story) has a primitive to call against each candidate HIS dir

## Acceptance Criteria

- [ ] **Given** a mock Unix socket at `$XDG_RUNTIME_DIR/hypr/{his}/.socket.sock` that responds to `activewindow\n` with a real window block, **When** I call `probe_instance(his)`, **Then** it returns `Liveness::LiveWithClients` within the 1s deadline
- [ ] **Given** a mock socket that responds with the literal `Invalid` (Hyprland's empty-state reply), **When** I call `probe_instance(his)`, **Then** it returns `Liveness::LiveEmpty`
- [ ] **Given** a HIS dir with no socket file, **When** I call `probe_instance(his)`, **Then** it returns `Liveness::Dead` (connection refused)
- [ ] **Given** a mock socket that accepts the connection but never replies, **When** I call `probe_instance(his)`, **Then** it returns `Liveness::Dead` after the 1s deadline (not after some longer default)
- [ ] **Given** the function is called concurrently against 4 mock instances, **When** all probes complete, **Then** total wall time is bounded by the slowest single probe (concurrent execution, not serial)

## Technical Notes

- Implement using `tokio::time::timeout(Duration::from_secs(1), …)` around the connect+request+read sequence
- The "request": `socket.write_all(b"activewindow\n").await`. The "reply": one line via `BufReader::read_line` (matching how `_hypr_cmd` is used elsewhere in the codebase — see `connect_hyprland_socket` for precedent on tokio::net::UnixStream usage)
- `Invalid` is a literal string Hyprland emits for empty workspaces; treat it as a substring match against the trimmed first line, not exact equality (Hyprland may add trailing whitespace/newlines)
- Mock-socket helper goes in `crates/media-control-lib/src/test_helpers.rs` — add a `MockHyprlandInstance` builder that takes a temp `XDG_RUNTIME_DIR`, an HIS string, and a response policy (`LiveWithClients` / `LiveEmpty` / `Hang` / `Refuse`). Reuse the existing env-mutex infrastructure in `test_helpers.rs` (per intent 015 single-source rule)

## Dependencies

### Requires

- None (first story of the unit)

### Enables

- 002-resolve-live-instance (uses `probe_instance` to evaluate candidates)
- 003-runtime-socket-path-uses-resolver (transitively)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| HIS dir exists but `.socket.sock` is a regular file (not a socket) | `Dead` — connect fails with `ENOTSOCK`-ish error; no panic |
| HIS dir is a symlink to a real instance | `Dead` — refuse to follow; matches the security posture of `create_fifo_at` in the daemon (lstat-based rejection of symlinks). Document the rationale; if it bites, relax in a follow-up |
| Permission denied opening the socket | `Dead` — and a `debug!` log line with the errno. Don't `error!` (legitimate in multi-user containers) |
| `activewindow` reply is empty bytes (0 bytes read before EOF) | `LiveEmpty` — server existed long enough to close the connection without data; semantically matches "alive but nothing to show" |

## Out of Scope

- Choice of how `Liveness` is exposed publicly vs. crate-internal (design-stage decision in bolt 028)
- Caching probe results across calls (intentionally not cached — each `resolve_live_his` call re-probes)
- Probing socket2 (event stream); we only probe socket (request/reply) because event stream has no defined "is alive" reply
