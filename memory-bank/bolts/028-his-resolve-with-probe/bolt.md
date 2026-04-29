---
id: 028-his-resolve-with-probe
unit: 001-his-resolve-with-probe
intent: 017-daemon-his-autodetect
type: simple-construction-bolt
status: complete
stories:
  - 001-probe-instance
  - 002-resolve-live-instance
  - 003-runtime-socket-path-uses-resolver
created: 2026-04-29T08:10:00Z
started: 2026-04-29T08:20:00Z
completed: 2026-04-29T10:50:00Z
requires_bolts: []
enables_bolts: [029-daemon-reconnect-re-resolution]
requires_units: []
blocks: false
current_stage: done
stages_completed:
  - name: plan
    completed: 2026-04-29T08:25:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-04-29T08:40:00Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-04-29T08:55:00Z
    artifact: test-walkthrough.md

complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 0
  testing_scope: 2
---

## Bolt: 028-his-resolve-with-probe

### Objective

Land probe-based HIS resolution in `media-control-lib::hyprland`. Add a `Liveness` enum, `probe_instance()` and `resolve_live_his()` functions, and replace `runtime_socket_path()`'s body to call them. End state: every CLI command and the daemon's outer reconnect path automatically pick the live Hyprland instance, with `HYPRLAND_INSTANCE_SIGNATURE` honored when its target is alive and a `warn!` when it's stale.

### Stories Included

- [ ] **001-probe-instance** — Implement `probe_instance(his) -> Liveness` with the
  `LiveWithClients | LiveEmpty | Dead` outcomes. Build the `MockHyprlandInstance` test
  helper in `test_helpers.rs` (response policies: `LiveWithClients`, `LiveEmpty`,
  `Hang`, `Refuse`). 1s probe deadline via `tokio::time::timeout`. Concurrent-probe
  test asserting wall time bounded by slowest single probe.

- [ ] **002-resolve-live-instance** — Implement `resolve_live_his(env_hint) -> Result<String, _>`.
  Apply the precedence rules: env-named-and-live wins (FR-2 fast path), env-named-and-dead
  triggers `warn!` and falls through (FR-3), scan picks `LiveWithClients` over `LiveEmpty`
  over nothing (FR-1), no-live returns env hint or first dir for backoff loop (FR-5).
  Concurrent probes via `JoinSet`. Add typed `HyprlandError` variant for the no-live case.
  Test matrix from story 002's acceptance criteria (7 cases).

- [ ] **003-runtime-socket-path-uses-resolver** — Replace `runtime_socket_path()`'s body
  to call `resolve_live_his(env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().as_deref())` and
  join with the requested basename. Treat empty-string env as None. Verify CLI integration
  tests pass unmodified. Manually reproduce the 2026-04-29 incident and confirm both CLI
  and daemon connect to the live instance.

### Expected Outputs

- New `Liveness` enum, `probe_instance`, `resolve_live_his` in
  `crates/media-control-lib/src/hyprland.rs` (or a new `hyprland/resolve.rs` if the file
  is getting unwieldy — design-stage decision)
- `runtime_socket_path` body replaced; signature unchanged
- New mock-instance scaffolding in `crates/media-control-lib/src/test_helpers.rs`
- New typed variant on `HyprlandError` for "no live Hyprland reachable"
- `cargo build --workspace` clean
- `cargo test --workspace` clean (existing tests pass; new tests cover the matrix)
- `cargo clippy --workspace -- -D warnings` clean

### Dependencies

None. This is the first bolt in the intent.

### Notes

Design-stage decisions for the bolt:

- **Module placement**: keep new code in `hyprland.rs` or split into `hyprland/resolve.rs`?
  `hyprland.rs` already contains the IPC client; if it's growing past comfort, split.
- **`Liveness` exposure**: `pub` or `pub(crate)`? CLI doesn't need the type; daemon doesn't
  need it post-Unit-1. Recommend `pub(crate)`, expose only `resolve_live_his()`.
- **mtime tiebreaker** (story 002 edge case "two `LiveWithClients`"): use `std::fs::metadata`
  on the dir entry; pick newest. Not the socket file's mtime — the dir's, because Hyprland
  creates the dir at instance startup and it's a stable signal.
- **Log level for the multi-seat-honored-env case**: `debug!` (story 002 AC) — not noisy
  enough to merit `info`, but a multi-seat user debugging avoidance behavior will want
  this in `RUST_LOG=debug`.

The probe cost matters. The acceptance budget (< 100ms with 4 instances) requires
concurrent probing — serial would push 4×1s worst case. Use `JoinSet` so the slowest
probe bounds the resolve time, not the sum.
