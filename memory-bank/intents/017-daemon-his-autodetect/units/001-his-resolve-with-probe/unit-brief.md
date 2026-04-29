---
unit: 001-his-resolve-with-probe
intent: 017-daemon-his-autodetect
phase: inception
status: ready
created: 2026-04-29T08:10:00Z
updated: 2026-04-29T08:10:00Z
---

# Unit Brief: his-resolve-with-probe

## Purpose

Replace `runtime_socket_path()`'s blind env-var lookup with a probe-based resolver that picks a *live* Hyprland instance when one exists. Substrate work in `media-control-lib::hyprland` so CLI and daemon both benefit through a single change point.

## Scope

### In Scope

- New types: `Liveness { LiveWithClients, LiveEmpty, Dead }` (or equivalent — design-stage decision)
- New functions in `media-control-lib::hyprland`:
  - `probe_instance(his: &str) -> Liveness` — connect to `.socket.sock`, send `activewindow`, classify reply
  - `resolve_live_his(env_hint: Option<&str>) -> Result<String, HyprlandError>` — apply FR-1/2/3/5 precedence
- Replace body of `runtime_socket_path(socket_basename: &str)` to call `resolve_live_his(env_hint=env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().as_deref())` and join with `socket_basename`. Signature unchanged.
- New mock-socket helpers in `crates/media-control-lib/src/test_helpers.rs` for spinning up fake `activewindow`-responding sockets at synthetic HIS dirs (per intent 015 FR-5: single-source mocks)
- Tests covering all FR-1 acceptance-criteria cases plus FR-2 (env wins when live), FR-3 (warn + fall through when env dead), FR-5 (no live → return env hint or first dir for backoff loop to handle)

### Out of Scope

- Changing the daemon's inner connect-retry loop (Unit 2)
- Changing `connect_hyprland_socket()` directly — it goes through `runtime_socket_path()` already
- Changing `socket2.sock` event-stream handling
- Periodic re-probing of the chosen HIS while connected (intentional non-goal — FR-4 covers reconnect, not steady-state)
- Caching the resolved HIS across calls — every CLI invocation does its own fresh resolve (the probe is cheap)

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Probe-based HIS resolution at startup | Must |
| FR-2 | Honor explicit `HYPRLAND_INSTANCE_SIGNATURE` when live | Must |
| FR-3 | Fall through to autodetection when env-named instance is dead | Must |
| FR-5 | Conservative fallback when no live instance is found | Must |

---

## Domain Concepts

### Key Entities

| Entity | Description | Attributes |
|--------|-------------|------------|
| `Liveness` | Probe outcome for one HIS instance | `LiveWithClients` (responsive socket, `activewindow` returned a real window), `LiveEmpty` (responsive socket, `activewindow` returned `Invalid`), `Dead` (refused/timeout) |
| HIS dir | A `$XDG_RUNTIME_DIR/hypr/{his}/` entry | Contains `.socket.sock`, `.socket2.sock`, `hyprland.lock`, `hyprland.log` |
| Resolution precedence | The decision rule applied by `resolve_live_his()` | env-named live → env-named; else best-of(live-with-clients > live-empty); else env hint (if any) for backoff loop; else first dir; else error |

### Key Operations

| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| `probe_instance(his)` | Connect to `$XDG_RUNTIME_DIR/hypr/{his}/.socket.sock` with a 1s deadline, send `activewindow\n`, read reply, classify | HIS string | `Liveness` |
| `resolve_live_his(env_hint)` | Apply precedence rules above | `Option<&str>` from env | `Result<String, HyprlandError>` |
| `runtime_socket_path(basename)` | Joined path for caller to open | basename like `.socket2.sock` | `Result<PathBuf, HyprlandError>` (signature unchanged) |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 3 |
| Must Have | 3 |
| Should Have | 0 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-probe-instance | `probe_instance()` + `Liveness` enum + mock-socket tests | Must | Planned |
| 002-resolve-live-instance | `resolve_live_his()` precedence rules + matrix tests | Must | Planned |
| 003-runtime-socket-path-uses-resolver | Replace `runtime_socket_path()` body; warn-on-stale-env log; integration tests | Must | Planned |

---

## Dependencies

### Depends On

| Unit | Reason |
|------|--------|
| (none) | Pure substrate addition |

### Depended By

| Unit | Reason |
|------|--------|
| 002-daemon-reconnect-re-resolution | Inner connect loop calls the new resolver |

### External Dependencies

| System | Purpose | Risk |
|--------|---------|------|
| Hyprland `.socket.sock` `activewindow` reply format | Liveness classification depends on `Invalid` literal vs. window data | Low — assumption documented; fallback if format changes is to treat any reply as live (degrades to FR-1 ambiguity, not silent failure) |

---

## Technical Context

### Suggested Technology

- Reuse `tokio::net::UnixStream` (already used in `connect_hyprland_socket`) for probing
- Use `tokio::time::timeout` for the 1s per-probe deadline
- Use `futures::future::join_all` (or `tokio::task::JoinSet`) for concurrent probes across multiple HIS dirs

### Integration Points

| Integration | Type | Protocol |
|-------------|------|----------|
| `.socket.sock` of each HIS dir | Local Unix socket | Hyprland IPC (request `activewindow`, read line) |

### Data Storage

None. Probe results are computed-and-discarded per resolution call.

---

## Constraints

- No new runtime dependencies
- Probe must time out (≤ 1s per instance) so a wedged Hyprland cannot block CLI or daemon startup
- Total resolution time < 100ms with up to 4 instances (concurrent probes)
- Mock infrastructure goes in existing `test_helpers.rs`, not a new module

---

## Success Criteria

### Functional

- [ ] `probe_instance()` correctly classifies live-with-clients, live-empty, refused, and timeout cases (mock-socket tests)
- [ ] `resolve_live_his()` honors env-var when its target probes alive (FR-2 test)
- [ ] `resolve_live_his()` falls through to scan when env-var probes dead, with `warn!` line naming the stale HIS (FR-3 test)
- [ ] `resolve_live_his()` prefers `LiveWithClients` over `LiveEmpty` over `Dead` (FR-1 test)
- [ ] When no live instance exists, returns env hint (or first HIS dir) so the caller's existing retry loop can take over (FR-5 test)
- [ ] Manual reproduction: with two HIS dirs (one empty, one with windows), CLI commands and daemon startup both connect to the windowed instance regardless of `HYPRLAND_INSTANCE_SIGNATURE` value

### Non-Functional

- [ ] Probe timeout ≤ 1s per instance (test asserts on a non-responsive mock)
- [ ] Total resolution time < 100ms with 4 mock instances (benchmark or timing assertion)
- [ ] Single-instance host: behavior indistinguishable from today (existing CLI tests pass; daemon startup time within noise)

### Quality

- [ ] All new code has unit tests in `test_helpers.rs`-backed style (no parallel mock layer)
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `cargo test --workspace` clean

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 028-his-resolve-with-probe | simple | 001, 002, 003 | Land the resolver and migrate `runtime_socket_path()` in one bolt — the three stories are tightly coupled and share test infrastructure |

---

## Notes

- The bolt's design stage should pick the exact `Liveness` shape (enum vs. struct with fields, whether to expose mtime tiebreaker) and the exact log level/format for the FR-3 stale-env warning. These are bikeshed-able details where construction context (other code in `hyprland.rs`) will resolve them faster than inception speculation.
- The FR-4 assumption (Hyprland-process death cleanly EOFs socket2 readers) is *not* validated by this unit — that's Unit 2's job.
