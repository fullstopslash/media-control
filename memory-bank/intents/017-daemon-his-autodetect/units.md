---
intent: 017-daemon-his-autodetect
phase: inception
status: units-decomposed
updated: 2026-04-29T08:10:00Z
---

# Daemon Live-HIS Resolution - Unit Decomposition

## Units Overview

This intent decomposes into **2 units**, executed sequentially. Unit 1 ships the entire fix for CLI and the daemon-startup case (because the daemon's existing reconnect loop already calls `runtime_socket_path()` afresh on each session). Unit 2 closes the smaller gap inside the inner connect-retry loop so a Hyprland restart during backoff also gets picked up.

### Unit 1: 001-his-resolve-with-probe

**Description**: Add a probe-based HIS resolver to `media-control-lib::hyprland`. `probe_instance(his) -> Liveness` enumerates `LiveWithClients | LiveEmpty | Dead` by connecting to the instance's `.socket.sock` and issuing `activewindow`. `resolve_live_his(env_hint: Option<&str>) -> Result<String, _>` implements the precedence rules in FR-1, FR-2, FR-3, FR-5: prefer the env-named instance if alive (FR-2), else scan dirs and pick the best candidate (FR-1), warn on stale env (FR-3), fall back to env-or-first-dir if nothing probes alive (FR-5). `runtime_socket_path()` becomes a thin wrapper over `resolve_live_his()`. CLI commands and the daemon's startup path benefit immediately because both already route through `runtime_socket_path()`.

**Stories**:

- 001-001-probe-instance: Add the `Liveness` enum and `probe_instance()` function with mock-socket tests covering live-with-clients, live-empty, refused, timeout.
- 001-002-resolve-live-instance: Add `resolve_live_his()` with the precedence rules and warn-on-stale-env behavior. Tests cover the matrix from FR-1 acceptance criteria (3 dirs / 1 dir / 0 dirs × env-set / env-unset / env-stale).
- 001-003-runtime-socket-path-uses-resolver: Replace `runtime_socket_path()`'s body so it calls `resolve_live_his()` once and joins the chosen HIS dir with the requested socket basename. Confirm CLI integration tests pass unchanged. Confirm a manual reproduction of the 2026-04-29 incident now connects to the live instance.

**Deliverables**:

- New `Liveness` enum + `probe_instance()` + `resolve_live_his()` in `media-control-lib::hyprland`
- `runtime_socket_path()` body replaced; signature unchanged
- New mock-socket helpers in `test_helpers.rs` (per intent 015 FR-5 single-source rule)
- All existing CLI and daemon tests pass

**Dependencies**:

- Depends on: None
- Depended by: 002-daemon-reconnect-re-resolution

**Estimated Complexity**: M

---

### Unit 2: 002-daemon-reconnect-re-resolution

**Description**: The daemon's `connect_hyprland_socket()` (in `media-control-daemon/src/main.rs`) has an inner exponential-backoff retry loop that resolves the socket path **once** before the loop. After Unit 1, the *outer* reconnect loop (in `run_event_session`) already re-resolves correctly. But during the inner loop's backoff, if Hyprland is restarted into a new HIS, the inner loop keeps hammering the old path until it eventually exhausts (it doesn't — it's an infinite retry today). Make the inner loop re-call `resolve_live_his()` on each iteration so a Hyprland restart during reconnect is picked up within one retry tick.

**Stories**:

- 002-001-connect-loop-re-resolves: Move the path resolution inside `connect_hyprland_socket()`'s loop body. Add a test (using mock socket dirs) that proves: start retry loop with no live instance → swap a live instance into the runtime dir mid-loop → next iteration finds it.

**Deliverables**:

- `connect_hyprland_socket()` re-resolves per iteration
- Reconnect-during-Hyprland-restart manually validated
- New unit test exercising the swap-mid-retry case

**Dependencies**:

- Depends on: 001-his-resolve-with-probe (uses the new resolver)
- Depended by: None

**Estimated Complexity**: S

---

## Requirement-to-Unit Mapping

- **FR-1** (Probe-based HIS resolution at startup) → 001-his-resolve-with-probe
- **FR-2** (Honor explicit `HYPRLAND_INSTANCE_SIGNATURE` when live) → 001-his-resolve-with-probe
- **FR-3** (Fall through to autodetection when env-named is dead) → 001-his-resolve-with-probe
- **FR-4** (Re-resolve on reconnect) → 002-daemon-reconnect-re-resolution (the outer-loop case is already covered by Unit 1; this unit closes the inner-loop gap)
- **FR-5** (Conservative fallback when no live instance found) → 001-his-resolve-with-probe

## Unit Dependency Graph

```text
[001-his-resolve-with-probe] ──> [002-daemon-reconnect-re-resolution]
```

Strict linear chain. Unit 2 imports the resolver Unit 1 produces.

## Execution Order

1. **001-his-resolve-with-probe** — substrate work; lands the entire user-visible fix because CLI is one-shot and the daemon's outer reconnect loop already re-calls `runtime_socket_path()`.
2. **002-daemon-reconnect-re-resolution** — small daemon-side cleanup that closes the inner-loop edge case so a Hyprland restart at any moment is recoverable, not just at session boundaries.
