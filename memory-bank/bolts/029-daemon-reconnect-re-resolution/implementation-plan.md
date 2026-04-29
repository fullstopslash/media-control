---
stage: plan
bolt: 029-daemon-reconnect-re-resolution
created: 2026-04-29T11:05:00Z
---

## Implementation Plan: daemon-reconnect-re-resolution (rescoped)

### Objective

Lock in FR-4's inner-loop swap-mid-retry behavior with an automated regression
test, and create a written record of the manual-validation outcome (whether
socket EOF fires reliably on Hyprland `kill -9`).

The production code change this story originally tracked is already in place
(see bolt 028's walkthrough). This plan describes only what remains.

### Deliverables

1. **`media-control-lib`: expose `test_helpers` cross-crate**
   - Change `#[cfg(test)] pub mod test_helpers;` to `#[cfg(any(test,
     feature = "test-helpers"))] pub mod test_helpers;` in
     `crates/media-control-lib/src/lib.rs`.
   - Add `[features] test-helpers = []` to
     `crates/media-control-lib/Cargo.toml`.
   - No behavioral change to the lib's public API or any runtime path.

2. **`media-control-daemon`: activate the feature for tests**
   - Add `[dev-dependencies] media-control-lib = { workspace = true,
     features = ["test-helpers"] }` to
     `crates/media-control-daemon/Cargo.toml`.

3. **`media-control-daemon`: swap-mid-retry test**
   - New `#[tokio::test]` in `crates/media-control-daemon/src/main.rs`'s
     `mod tests` (or a new `mod connect_loop_tests` if preferred for
     separation).
   - Test scaffolding:
     - Use `with_isolated_runtime_dir` from the lib's test_helpers to install
       a tempdir XDG_RUNTIME_DIR and clear `HYPRLAND_INSTANCE_SIGNATURE`.
     - Spawn `connect_hyprland_socket()` as a tokio task. With no `hypr/`
       subdir under the tempdir, the resolver returns `NoLiveInstance` →
       `connect_hyprland_socket` warns and backs off.
     - Sleep `~600ms` (one full retry tick at the 500ms initial backoff +
       slack) to ensure at least one loop iteration has completed and the
       task is mid-backoff.
     - Install a `MockHyprlandInstance` with `InstancePolicy::LiveWithClients`
       at a known HIS under `${tempdir}/hypr/`. **Important**: the daemon's
       `connect_hyprland_socket` calls `runtime_socket_path(".socket2.sock")`,
       so the mock must serve `.socket2.sock` at the expected path. Today
       `MockHyprlandInstance` serves a probe-respondent socket but bolt 028
       used it primarily for `activewindow` probes against `.socket.sock`.
       Plan: extend `MockHyprlandInstance` to optionally also bind a
       `.socket2.sock` listener, OR add a sibling helper
       `MockHyprlandInstance::with_socket2(...)`. Decision deferred to Stage 2;
       prefer the smallest delta.
     - Await the spawned task with a `tokio::time::timeout` of ~5s. Expected:
       it returns `Ok(stream)` once the next backoff tick fires post-mock.
   - Acceptance: test passes deterministically. Backoff is 500ms → 1s → 2s …,
     so worst-case observed wall time should be < 3s.

4. **Construction-log: manual-validation entry**
   - Run `media-control-daemon` against a real Hyprland on a test seat.
   - `kill -9 $(pidof Hyprland)`; restart Hyprland; observe daemon journal.
   - Capture two journal lines: the `warn!` after socket EOF, and the
     subsequent `info!("Connected to Hyprland socket at ...")` against the
     new HIS.
   - Append findings to
     `memory-bank/intents/017-daemon-his-autodetect/units/002-daemon-reconnect-re-resolution/construction-log.md`.
   - If EOF does not fire (kernel keeps connection in CLOSE_WAIT, daemon
     stays blocked indefinitely): record the negative result, do **not**
     extend bolt scope, propose intent 018-daemon-heartbeat for follow-up.

### Dependencies

- Bolt 028 must be merged (verified — `b3c108d4ff8b` on main 2026-04-29).
- `MockHyprlandInstance` exists with `LiveWithClients` policy (verified —
  `crates/media-control-lib/src/test_helpers.rs:478`).
- `with_isolated_runtime_dir` exists (referenced by bolt 028's
  test-walkthrough notes about deadlock fix).

### Technical Approach

**Why a feature flag and not a separate test-utils crate?** A separate crate
would be the more "library-style" answer but adds workspace and dependency
weight for a single mock module. Conditional compilation behind a feature is
the conventional Rust pattern for sharing test helpers across crates without
exposing them to downstream consumers (the feature is only enabled by the
daemon's dev-dependencies, never by a release build).

**Why not put the test in the lib crate?** Because `connect_hyprland_socket`
lives in the daemon binary crate. Pulling it into the lib would change the
architectural boundary that intent 015 (avoider carve-out) deliberately
established.

**Why the test needs `.socket2.sock` and not just `.socket.sock`?**
`probe_instance` (the resolver's liveness probe) connects to `.socket.sock`
to send `activewindow`. `connect_hyprland_socket` connects to `.socket2.sock`
to read events. The swap-mid-retry test needs *both* endpoints to be
probe-live and connect-acceptable on the mock instance — otherwise the
resolver classifies the mock as `Dead` and the test's success path never
fires. Concretely: probe → `.socket.sock` (existing); connect →
`.socket2.sock` (new for this test). Mock extension must serve both.

### Acceptance Criteria

- [ ] `cargo test --workspace` clean (existing 402 tests + 1 new daemon test
      = 403)
- [ ] `cargo clippy --workspace --tests -- -D warnings` clean
- [ ] `cargo build --workspace` clean (release-build path unaffected by the
      `test-helpers` feature)
- [ ] Construction-log updated with manual-validation findings or a
      documented negative result + follow-up-intent reference
- [ ] No new dependencies pulled into release builds
