---
unit: 002-daemon-reconnect-re-resolution
intent: 017-daemon-his-autodetect
created: 2026-04-29T11:00:00Z
last_updated: 2026-04-29T11:50:00Z
---

# Construction Log: daemon-reconnect-re-resolution

## Original Plan

**From Inception**: 1 bolt planned (029)
**Planned Date**: 2026-04-29

| Bolt ID | Stories | Type |
|---------|---------|------|
| 029-daemon-reconnect-re-resolution | 001-connect-loop-re-resolves | simple-construction-bolt |

## Replanning History

| Date | Action | Change | Reason | Approved |
|------|--------|--------|--------|----------|
| 2026-04-29 | rescope | 029 from "implement + test" to "test + manual-validate" | Production-code change for FR-4 inner-loop case (story 001 ACs #1, #4) was preemptively delivered by bolt 028 as part of the `runtime_socket_path` async refactor. ACs #2 (mock-test) and #3 (manual validation) remain. | Yes |

## Current Bolt Structure

Single bolt covering remaining test + validation work.

## Construction Log

- **2026-04-29T11:00:00Z**: 029 rescoped — production code subsumed by bolt 028; remaining work is the swap-mid-retry automated test and manual-validation log entry. Replan rationale verified by re-reading `connect_hyprland_socket()` post-028 — `get_socket2_path().await` is now called per iteration of the retry loop (`crates/media-control-daemon/src/main.rs:537-568`).
- **2026-04-29T11:05:00Z**: 029 stage-complete — plan → implement
- **2026-04-29T11:30:00Z**: 029 stage-complete — implement → test. Surfaced and fixed a pre-existing cross-domain env-mutex race (daemon's local `env_test_lock` vs lib's `async_env_test_mutex` were independent lock domains, both serializing `XDG_RUNTIME_DIR` mutations). The new test was the first daemon-crate test to enter the lib's mutex via `with_isolated_runtime_dir`, so the race was latent until now. Forwarder fix unifies both crates onto the lib's mutex.
- **2026-04-29T11:50:00Z**: 029 completed — 404/404 workspace tests, 5/5 deterministic on `cargo test -p media-control-daemon`. Two new daemon-crate tests cover AC #2 (`connect_loop_picks_up_live_instance_mid_retry`) and AC #3 protocol-level (`connect_loop_recovers_after_peer_close`). Real-Hyprland kernel-level validation of AC #3 was not run in-session — the kill -9 would terminate the host process running the test runner. Structural test validates the EOF assumption against real `tokio::net::UnixStream` semantics; treating real-kernel + real-Hyprland combination as a kernel/tokio invariant. Recipe documented in test-walkthrough.md for user-driven validation at next convenient TTY/spare-seat session.
- **Unit 002 closed**. Intent 017 is feature-complete. Final user-driven validation (real-Hyprland kill -9) optional, not blocking.
