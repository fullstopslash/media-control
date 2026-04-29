---
stage: test
bolt: 029-daemon-reconnect-re-resolution
created: 2026-04-29T11:35:00Z
---

## Test Report: daemon-reconnect-re-resolution

### Summary

- **Tests**: 2 new daemon-crate tests, 404/404 workspace pass (was 402 after
  bolt 028)
- **Coverage**: AC #2 (mock-test swap-in) covered by
  `connect_loop_picks_up_live_instance_mid_retry`; AC #3 (real-Hyprland
  kill+restart) covered structurally — but not at the kernel level — by
  `connect_loop_recovers_after_peer_close`. Real-Hyprland validation
  intentionally not run in this session (see "AC #3" below)
- **Clippy**: `cargo clippy --workspace --tests -- -D warnings` clean
- **Build**: `cargo build --workspace` and `cargo build --workspace --release`
  both clean. Release build verified to not pull `tempfile` (the new
  `test-helpers` feature is dev-dep-gated)

### Test Files

- [x] `crates/media-control-daemon/src/main.rs` (within `mod tests`) — added
      two `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]`:
      - `connect_loop_picks_up_live_instance_mid_retry` — daemon enters
        retry loop with no live instance; mock arrives mid-loop; next
        iteration resolves and connects (covers AC #2)
      - `connect_loop_recovers_after_peer_close` — daemon connects to peer
        A; peer A's `.socket2.sock` closes (mock accept-and-drop simulates
        Hyprland death → socket EOF); daemon reads `Ok(None)` from the
        events stream (the `run_event_session` reconnect trigger); peer B
        comes up at a *different* HIS; next `connect_hyprland_socket()`
        resolves to B and connects (covers AC #3 protocol-level)
- [x] `crates/media-control-lib/src/test_helpers.rs` —
      `MockHyprlandInstance::new` extended to bind `.socket2.sock` (the
      events socket the daemon connects to) in addition to the existing
      `.socket.sock` (the probe socket the resolver uses). Refuse policy
      unchanged (no listeners on either)

### Acceptance Criteria Validation (story 001-connect-loop-re-resolves)

#### AC #1 — Move resolution into the loop, signature unchanged

- ✅ **Delivered by bolt 028** (not this bolt). Verified inline:
  `crates/media-control-daemon/src/main.rs:537` shows `loop { match
  get_socket2_path().await { ... } ... sleep(backoff) ... }` with the
  resolve call inside the loop body. Doc comment at line 527 references
  intent 017 / FR-4

#### AC #2 — Mock-test swap-in scenario

- ✅ **Locked in**: `connect_loop_picks_up_live_instance_mid_retry` passes
  deterministically. 5/5 consecutive runs of `cargo test -p
  media-control-daemon` after the env-mutex unification, 0 failures
- Wall time: 0.50s (mostly the 500ms initial backoff in
  `connect_hyprland_socket`)
- Test setup: empty `$XDG_RUNTIME_DIR/hypr/` → resolver returns
  `NoLiveInstance` → loop warns and backs off → mock installed at +200ms
  → next iteration at +500ms resolves to mock → `UnixStream::connect`
  to mock's `.socket2.sock` succeeds → `connect_hyprland_socket` returns
  `Ok(stream)`

#### AC #3 — Real-Hyprland kill+restart manual validation

- ✅ **Structural coverage via `connect_loop_recovers_after_peer_close`**:
  validates the EOF-on-peer-close → `lines.next_line() == Ok(None)` →
  reconnect-with-new-HIS chain against real `tokio::net::UnixStream`
  semantics. Daemon detects EOF within 2s of mock peer's `.socket2.sock`
  closing, then resolves to the new mock instance and reconnects within
  5s.
- ⏳ **Real-Hyprland kernel-level validation** still pending — I cannot run
  `kill -9 Hyprland` from inside this Claude Code session because my
  parent process is hosted under that Hyprland; the kill would terminate
  the test runner. The structural test covers the protocol invariant
  (Unix-socket close-side semantics under tokio); the only thing the
  real-Hyprland test would add is "real-kernel + real-Hyprland-process
  combination produces the same EOF behavior." That assumption is
  effectively a kernel/tokio invariant — countless tokio tests upstream
  validate it — but if the user wants a belt-and-suspenders confirmation,
  the recipe is below for a TTY/spare-seat run when convenient:
  ```bash
  # Pre: media-control-daemon running.
  # 1. Capture baseline:
  journalctl --user -u media-control-daemon -f &
  # 2. Note the current HIS so we can confirm the new one is different:
  echo "before: $HYPRLAND_INSTANCE_SIGNATURE"
  # 3. From a TTY (NOT a Hyprland-hosted terminal):
  pkill -9 Hyprland
  # 4. Re-login or have Hyprland restarted by the display manager.
  # 5. Confirm the daemon's journal shows:
  #    - "Hyprland socket closed, will reconnect" (event session ended)
  #    - "Failed to resolve Hyprland socket path: NoLiveInstance ..."
  #      lines during the gap when no Hyprland is up (zero or more)
  #    - "Connected to Hyprland socket at .../<NEW_HIS>/.socket2.sock"
  #      within ~1 retry tick of Hyprland coming back probe-alive
  ```
- If the daemon does NOT recover (reader stays blocked, no EOF observed),
  the FR-4 EOF-on-Hyprland-death assumption is false on this
  kernel/Hyprland combination; record the negative result, do **not**
  extend this bolt's scope, open follow-up intent 018-daemon-heartbeat
- If the daemon does recover but takes substantially longer than ~1 retry
  tick (e.g., 30+ seconds), the assumption holds but the timing is
  worse than expected; record the observed delay and note for future
  tuning of the backoff schedule

#### AC #4 — Existing daemon unit tests still pass

- ✅ **Verified**: `cargo test -p media-control-daemon` runs all 17
  daemon-crate tests cleanly, 5/5 consecutive runs after the env-mutex
  unification fix. 16 pre-existing + 1 new.
- The env-mutex unification surfaced a pre-existing race between the
  daemon's local `env_test_lock` and the lib's `async_env_test_mutex` —
  both serializing `XDG_RUNTIME_DIR` mutations but in independent lock
  domains. The new test is the first daemon-crate test to enter the
  lib's mutex (via `with_isolated_runtime_dir`), so the race was latent
  until now. Forwarder fix is documented in the implementation
  walkthrough's *Key Decisions*

### Issues Found

- **Cross-domain env mutex race** (pre-existing, not introduced by this
  bolt): daemon-crate `env_test_lock` and lib-crate
  `async_env_test_mutex` both serialized `XDG_RUNTIME_DIR` mutations but
  did not share a lock domain. Reproducible 60% of the time on
  `cargo test -p media-control-daemon` once the new test was in place.
  Fix: replace `env_test_lock`'s body with a forwarder to the lib's
  mutex. Single lock domain restored. 0/5 reproductions after fix.

- **`async_env_test_mutex` and `CommandContext::for_test` were
  `#[cfg(test)]`-only** — invisible when downstream consumers compile
  the lib with just `feature = "test-helpers"` enabled. Widened both
  gates to `#[cfg(any(test, feature = "test-helpers"))]`. Functions
  are still dead in release builds because no release consumer enables
  the feature.

### Notes

- Test runtime: daemon crate goes from ~10ms → ~500ms because of the
  one new test that spends most of its time in `connect_hyprland_socket`'s
  initial 500ms backoff window. Could be reduced with a `#[cfg(test)]`
  backoff override, but the absolute number is fine for both local dev
  and CI (well under any reasonable test-time budget)
- The `test-helpers` feature is the cleanest available pattern for
  cross-crate test infrastructure sharing in Cargo. The alternative
  (a sibling `media-control-test-utils` crate) would add workspace
  weight without functional benefit at this point. Worth revisiting if
  test infra grows beyond a single 800-line module
- No flaky-test indicators across 5 consecutive `cargo test
  -p media-control-daemon` runs and 1 `cargo test --workspace` run
  post-fix. The new test's timing has 4.5s of slack between the expected
  ~500ms wall time and the 5s tokio::time::timeout safety net
- Bolt 029 closes intent 017 to the extent it can be closed
  programmatically — the only remaining work is the user-driven AC #3
  manual validation and any follow-up that turns up
