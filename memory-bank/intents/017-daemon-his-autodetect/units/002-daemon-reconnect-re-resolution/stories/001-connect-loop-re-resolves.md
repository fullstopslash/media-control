---
id: 001-connect-loop-re-resolves
unit: 002-daemon-reconnect-re-resolution
intent: 017-daemon-his-autodetect
status: ready
priority: should
created: 2026-04-29T08:10:00Z
assigned_bolt: 029-daemon-reconnect-re-resolution
implemented: false
---

# Story: 001-connect-loop-re-resolves

## User Story

**As a** user whose Hyprland gets restarted (crash + restart, manual `hyprctl dispatch exit` and re-login, etc.) while the daemon is running
**I want** the daemon's inner connect-retry loop to re-resolve the HIS on each retry tick instead of hammering the old path forever
**So that** the daemon recovers within one retry tick of Hyprland's new instance becoming probe-alive — no daemon restart required

## Acceptance Criteria

- [ ] **Given** `connect_hyprland_socket()` in `crates/media-control-daemon/src/main.rs`, **When** the path-resolution call is moved from before the loop into the loop body (each iteration calls `get_socket2_path()` afresh), **Then** the function still has the same signature and exponential-backoff schedule (500ms → 10s)
- [ ] **Given** a mock test setup with no live HIS dirs, **When** the daemon enters `connect_hyprland_socket()` and the loop has retried at least once, **And** I install a mock `LiveWithClients` HIS dir, **Then** the next loop iteration resolves to that new HIS and connects successfully
- [ ] **Given** a real Hyprland session, **When** I `kill` the Hyprland process and `Hyprland` is restarted (new HIS), **Then** the daemon journal shows reconnection to the new HIS within ~1 retry-tick of the new instance being probe-alive — not a stuck retry against the dead path
- [ ] **Given** existing daemon unit tests, **When** the change lands, **Then** they all pass (the per-iteration resolve does not change retry timing or error behavior)

## Technical Notes

- Today's structure (lines 526-554 of `crates/media-control-daemon/src/main.rs`):
  ```rust
  let socket_path = get_socket2_path()?;   // ← resolves once
  let mut backoff = …;
  loop {
      // tries `socket_path` repeatedly, never re-resolves
  }
  ```
- New structure:
  ```rust
  let mut backoff = …;
  loop {
      let socket_path = match get_socket2_path() {
          Ok(p) => p,
          Err(e) => { warn!("failed to resolve HIS: {e}"); /* keep backoff timing */ }
      };
      // try connect to socket_path
  }
  ```
- Each iteration's resolve adds the probe cost (≤ 100ms). Backoff starts at 500ms so the resolve overhead is bounded; backoff growing to 10s makes the resolve cost negligible at the high end
- Resolution failure inside the loop must NOT bypass the backoff — log the error, sleep the current backoff, double, retry. Treat resolve-failure identically to connect-failure for the purpose of the retry schedule
- The mock test reuses the `MockHyprlandInstance` builder added in story 001-probe-instance (Unit 1) — no new mock infrastructure

## Dependencies

### Requires

- All Unit 1 stories — `runtime_socket_path()` must already go through `resolve_live_his()`. Without that, per-iteration resolves still pick the stale env-named path

### Enables

- None (last story of the intent)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| HIS resolution fails inside the loop (no Hyprland reachable) | Log `warn!`, apply current backoff, retry. Don't return an error from `connect_hyprland_socket` — the function's contract is "eventually return a connected stream"; nothing should change that |
| Hyprland restarts twice during one backoff cycle | Each iteration re-resolves so we always converge on whatever HIS is currently live, not whichever was live first |
| `$XDG_RUNTIME_DIR/hypr/` is briefly empty during Hyprland restart | Resolution returns `Err(NoLiveInstance)`; loop applies backoff; subsequent iteration finds the new dir |
| Permission to a fresh HIS dir is briefly missing (uid race during instance startup) | Probe returns `Dead` for that dir; resolve falls through to env hint or `Err`; next iteration retries naturally |

## Out of Scope

- Adding heartbeat/ping while connected (separate concern; the FR-4 assumption is that socket EOF fires reliably on Hyprland death — validate empirically during this story but don't add heartbeating speculatively)
- Refactoring `connect_hyprland_socket` to share retry logic with other backoff-using code in the daemon (no other code; keep it local)
- Changing the backoff schedule (existing 500ms → 10s is reasonable; not in scope)
