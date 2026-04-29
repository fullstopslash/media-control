---
stage: implement
bolt: 029-daemon-reconnect-re-resolution
created: 2026-04-29T11:30:00Z
---

## Implementation Walkthrough: daemon-reconnect-re-resolution

### Summary

Locked in FR-4's swap-mid-retry behavior with an automated daemon-crate test
that exercises `connect_hyprland_socket()` end-to-end against a mock that
appears mid-loop. To do that without duplicating the lib's mock layer, exposed
the lib's `test_helpers` module behind a `test-helpers` feature flag and
extended `MockHyprlandInstance` to bind `.socket2.sock` (the events socket
the daemon connects to) in addition to the existing `.socket.sock` (the probe
socket the resolver uses). Also unified the daemon's local `env_test_lock`
onto the lib's `async_env_test_mutex` to eliminate a cross-domain race that
my new test surfaced on the very first run.

### Structure Overview

The lib crate now has a feature-gated `pub mod test_helpers` with an empty
`test-helpers` feature that pulls in `tempfile` as an optional dependency.
The daemon's dev-dependencies activate that feature, so the daemon's
`#[cfg(test)] mod tests` can `use media_control_lib::test_helpers::*` —
release builds never see this surface because dev-dep features are inert
outside the test profile.

`MockHyprlandInstance::new()` now binds two listeners per non-`Refuse`
policy: the existing `.socket.sock` server (probe-respondent, governed by
`InstancePolicy`) and a new `.socket2.sock` server (accept-and-drop,
matching what `connect_hyprland_socket` actually awaits — the connect
handshake, not the event stream).

### Completed Work

- [x] `crates/media-control-lib/Cargo.toml` — added `test-helpers` feature
      that activates an optional `tempfile` dep; doc-comment cites intent 015
      FR-5 as the rationale for keeping the mock layer single-source
- [x] `crates/media-control-lib/src/lib.rs` — gate widened from
      `#[cfg(test)]` to `#[cfg(any(test, feature = "test-helpers"))]`
- [x] `crates/media-control-lib/src/commands/shared.rs` — same gate widening
      for `async_env_test_mutex` (now `pub`, was `pub(crate)`) and
      `CommandContext::for_test`. Both must be visible when the lib is
      compiled with `test-helpers` on but `cfg(test)` off (downstream's
      dev-dep activation path)
- [x] `crates/media-control-lib/src/test_helpers.rs` — `MockHyprlandInstance`
      gained a `_socket2_server: Option<MockServerHandle>` field; `new()`
      binds `.socket2.sock` and spawns a small accept-and-drop loop alongside
      the existing `.socket.sock` server. Refuse policy unchanged (no
      listeners on either socket, matches "instance dir present but socket
      file absent")
- [x] `crates/media-control-daemon/Cargo.toml` — `[dev-dependencies]` now
      pulls `media-control-lib` with `features = ["test-helpers"]`; doc
      comment notes that release builds do not see the feature
- [x] `crates/media-control-daemon/src/main.rs` — added one
      `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`:
      `connect_loop_picks_up_live_instance_mid_retry`. Spawns
      `connect_hyprland_socket()` against an isolated tempdir
      (`with_isolated_runtime_dir` from the lib), sleeps 200ms into the
      first 500ms backoff, installs a `LiveWithClients` mock, awaits the
      connect to succeed within 5s. Inline `HisGuard` clears
      `HYPRLAND_INSTANCE_SIGNATURE` for the test scope — the
      `with_isolated_runtime_dir` mutex keeps env mutation safe
- [x] `crates/media-control-daemon/src/main.rs` — `env_test_lock()` body
      replaced from a local `OnceLock<Mutex>` to a forwarder calling
      `media_control_lib::commands::shared::async_env_test_mutex()`. Two
      lock domains were a latent bug; the new test made it visible
      (60% reproducible across 5 daemon-crate test runs before the fix,
      0/5 after)

### Key Decisions

- **Feature flag, not a separate test-utils crate**: a sibling crate would
  be the more "library-style" answer but adds workspace and dependency weight
  for a single mock module. Conditional compilation behind a feature is the
  conventional Rust pattern; the feature is only enabled by the daemon's
  dev-dependencies, never by release.

- **Bind `.socket2.sock` for every non-Refuse policy, not just
  `LiveWithClients`**: real Hyprland always serves both sockets regardless of
  whether any clients are open. Binding both for `LiveEmpty` and `Hang` too
  matches reality and costs nothing — none of the existing 17 lib-crate
  resolver tests asserted on `.socket2.sock` absence.

- **`.socket2.sock` server is accept-and-drop**: `connect_hyprland_socket`
  returns from its loop as soon as `UnixStream::connect` succeeds. It does
  not read from the stream inside that function. Synthesizing a Hyprland
  event protocol on top of `.socket2.sock` would be scope creep into testing
  `run_event_session` (which is out of scope for this bolt and unit).

- **`#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`**: the
  default single-threaded current-thread runtime would deadlock the test
  because the spawned `connect_task` and the main test future need to run
  concurrently — the main future has to install the mock *while*
  connect_task is mid-loop. Multi-thread with 2 workers is the minimum.

- **`HisGuard` is inline in the test, not a helper**: only one daemon test
  needs to clear `HYPRLAND_INSTANCE_SIGNATURE` today. If a second one
  appears, factor at that point.

- **Unifying `env_test_lock` onto the lib's mutex was not optional**: the
  daemon's existing tests hold a different `OnceLock<Mutex>` than the lib's
  `async_env_test_mutex`. With both crates now mutating `XDG_RUNTIME_DIR`
  in the same test binary, the two lock domains race. I caught this on the
  third overall workspace test run after writing the new test — the
  pre-existing `read_pid_file_rejects_low_pids` test failed with `left:
  None, right: Some(12345)` (the runtime dir was swapped underneath it).
  Forwarding to `async_env_test_mutex` gives both lib and daemon tests one
  lock domain.

### Deviations from Plan

**Did not extend `MockHyprlandInstance` with a builder method
(`with_socket2()` etc.)** — the plan listed this as a Stage-2 design call.
On reflection, an opt-in builder would mean the daemon test has to remember
to call it; an unconditional bind matches Hyprland reality and avoids the
foot-gun. Net delta is one extra listener per non-Refuse mock, which the
17 existing lib-crate resolver tests do not care about (they tested probe
classification only, not `.socket2.sock` state).

**Found and fixed an unrelated pre-existing bug**: the duplicate
env-mutex domain (daemon's local `env_test_lock` vs lib's
`async_env_test_mutex`) was already a race condition waiting to fire — it
just hadn't fired before because the lib's tests ran in their own binary
and the daemon's tests in another, never sharing a process. The new test
is the first daemon-crate test that goes through `with_isolated_runtime_dir`
into the lib's mutex, which is what made the race observable. Fixing the
race was not in this bolt's scope but is a hard prerequisite for the new
test to pass deterministically; documented and left in.

### Dependencies Added

- [x] `tempfile = { version = "3", optional = true }` in
      `media-control-lib/[dependencies]`. The lib's `[dev-dependencies]`
      copy is retained for `cargo test -p media-control-lib` builds where
      the feature isn't auto-activated. Cargo accepts the same crate in
      both sections; they unify.

### Developer Notes

- The test takes ~500ms in the steady state. The bulk of that is the 500ms
  initial backoff that `connect_hyprland_socket` enters on its first
  `NoLiveInstance` resolve. Could be made instant with a `#[cfg(test)]`
  override of the initial backoff, but the slower test exercises real
  timing and the absolute number is fine.
- `with_isolated_runtime_dir` only manages `XDG_RUNTIME_DIR`. Tests that
  also need to clear `HYPRLAND_INSTANCE_SIGNATURE` (or other env state)
  must do so inline inside the closure, where the env mutex is already
  held — re-acquiring `async_env_test_mutex` would deadlock (tokio's
  Mutex isn't reentrant). Same gotcha bolt 028's test-walkthrough flagged.
- The release build does NOT pull `tempfile` (verified — `cargo build
  --workspace --release` recompiled the crates without it). The
  `test-helpers` feature is dev-dep-gated so release consumers see no
  change to the lib's runtime dependency closure.
- A future test wanting to assert that the daemon survives a full
  EOF→reconnect→connect cycle (i.e., `run_event_session`'s outer loop)
  would need to add a real event-streaming `.socket2.sock` mock. Out of
  scope here; could be a follow-up bolt under intent 017 if motivation
  emerges, or punt to a later intent.
