---
unit: 002-daemon-reconnect-re-resolution
intent: 017-daemon-his-autodetect
phase: inception
status: ready
created: 2026-04-29T08:10:00Z
updated: 2026-04-29T08:10:00Z
---

# Unit Brief: daemon-reconnect-re-resolution

## Purpose

Close the only remaining gap after Unit 1: the daemon's inner exponential-backoff connect loop in `connect_hyprland_socket()` resolves the socket path once *before* the loop and then hammers that path forever. If Hyprland is restarted (new HIS) while the daemon is mid-reconnect, the inner loop never picks up the new instance. Move the resolve call inside the loop body so each retry tick re-resolves cheaply.

## Scope

### In Scope

- Modify `connect_hyprland_socket()` in `crates/media-control-daemon/src/main.rs` to call `runtime_socket_path(".socket2.sock")` (now backed by `resolve_live_his`) on each loop iteration instead of once before the loop
- Add a daemon-level test using mock socket dirs (built on Unit 1's `test_helpers.rs` additions) demonstrating: enter retry loop with no live instance → install a live-instance mock dir mid-loop → next iteration connects to it
- Manual validation: kill+restart Hyprland while daemon runs; daemon resumes within ~1 retry tick

### Out of Scope

- Changes to `run_event_session` or `run_event_loop` (the outer reconnect path is already correct after Unit 1)
- Changes to the resolver itself (Unit 1)
- Heartbeat-style "re-probe while connected" (the FR-4 assumption is that socket EOF reliably signals Hyprland death; this unit validates that assumption empirically as part of manual validation, but does not add heartbeating)

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-4 | Re-resolve on reconnect (inner-loop case; outer-loop case already covered by Unit 1's `runtime_socket_path` change) | Should |

---

## Domain Concepts

### Key Entities

| Entity | Description | Attributes |
|--------|-------------|------------|
| `connect_hyprland_socket()` | Daemon-local helper with an exponential-backoff retry loop (500ms → 10s) for connecting to socket2 | Currently resolves path once; this unit makes it resolve per iteration |

### Key Operations

| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| Per-retry resolve | Inside the connect loop, call `runtime_socket_path(".socket2.sock")` before each `UnixStream::connect` attempt | none | `PathBuf` |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 1 |
| Must Have | 0 |
| Should Have | 1 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-connect-loop-re-resolves | Move resolve call inside `connect_hyprland_socket()` loop body; add swap-mid-retry test | Should | Planned |

---

## Dependencies

### Depends On

| Unit | Reason |
|------|--------|
| 001-his-resolve-with-probe | This unit relies on the resolver introduced in Unit 1; without it, per-iteration resolves still pick the stale env-named path |

### Depended By

| Unit | Reason |
|------|--------|
| (none) | Last unit in the intent |

### External Dependencies

None beyond what Unit 1 introduces.

---

## Technical Context

### Suggested Technology

Reuse the mock-socket test scaffolding added by Unit 1 in `test_helpers.rs`.

---

## Constraints

- One callsite change. Avoid scope creep into refactoring `connect_hyprland_socket`'s structure.
- Per-iteration resolve cost must stay within the existing backoff budget (resolve is ≤ 100ms; backoff starts at 500ms, so resolve overhead is bounded and acceptable).

---

## Success Criteria

### Functional

- [ ] After socket EOF + Hyprland restart with new HIS, daemon reconnects to the new instance without restart
- [ ] Mock test demonstrates inner-loop swap-in case

### Non-Functional

- [ ] No measurable regression in single-instance reconnect time (the per-iteration resolve fast-paths through `LiveWithClients` for the env hint)

### Quality

- [ ] Existing daemon tests pass
- [ ] `cargo clippy -p media-control-daemon -- -D warnings` clean

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 029-daemon-reconnect-re-resolution | simple | 001 | Move resolve into the loop; add the swap-mid-retry test |

---

## Notes

- This unit is small. The reason it exists as a separate unit (not a fourth story in Unit 1) is ownership: Unit 1 is lib-substrate, this is daemon-application. Keeping the boundary clean matches intent 015's carve-out discipline.
- During manual validation, observe what happens when Hyprland is `kill -9`'d: confirm socket2 reader sees EOF promptly. If it doesn't (e.g., kernel keeps the connection in CLOSE_WAIT), the FR-4 assumption needs revisiting and a heartbeat may be necessary in a follow-up intent.
