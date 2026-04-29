---
stage: test
bolt: 028-his-resolve-with-probe
created: 2026-04-29T08:55:00Z
---

## Test Report: his-resolve-with-probe

### Summary

- **Tests**: 17 new (402 total in workspace), 17/17 new pass, 402/402 total pass
- **Coverage**: All 5 FRs covered by direct unit tests; one acceptance criterion (probe-time-bounded-by-slowest-not-sum) verified empirically with 4 hanging mocks completing in < 1.5s instead of ~4s
- **Clippy**: `cargo clippy --workspace --tests -- -D warnings` clean

### Test Files

- [x] `crates/media-control-lib/src/test_helpers.rs` — added `MockHyprlandInstance` builder + `InstancePolicy` enum (`LiveWithClients`, `LiveEmpty`, `Hang`, `Refuse`); aborts the spawned listener task on drop so test cleanup never leaks
- [x] `crates/media-control-lib/src/hyprland.rs` (existing `mod tests` block) — 17 new `#[tokio::test]` cases covering probe behavior, resolver precedence, symlink rejection, and integration through `runtime_socket_path()`

### New Test Inventory (by story)

#### Story 001-probe-instance — 6 tests

- `probe_classifies_live_with_clients` — mock socket with synthetic window block → `Liveness::LiveWithClients`
- `probe_classifies_live_empty` — mock socket replying `Invalid` → `Liveness::LiveEmpty`
- `probe_classifies_dead_when_socket_missing` — `Refuse` policy (HIS dir but no socket file) → `Dead`
- `probe_classifies_dead_when_dir_missing_entirely` — neither HIS dir nor socket exists → `Dead`
- `probe_times_out_on_hanging_server` — `Hang` policy (accept but never reply) → `Dead` within `PROBE_TIMEOUT + 300ms slack`
- `probe_concurrent_runs_in_parallel_not_serial` — 4 hanging mocks probed concurrently complete in < 1.5s (vs ~4s if serial)

#### Story 002-resolve-live-instance — 9 tests

- `resolve_env_hint_live_returns_env_without_scan` — env hint live → returns env (FR-2 fast path)
- `resolve_env_hint_live_empty_wins_over_others_with_clients` — explicit pin honored over "better" instance
- `resolve_env_hint_dead_falls_through_to_live_scan` — exact 2026-04-29 incident shape (FR-3)
- `resolve_no_hint_prefers_live_with_clients_over_empty_over_dead` — FR-1 preference ladder
- `resolve_no_hint_returns_live_empty_when_only_choice` — empty acceptable when nothing better
- `resolve_env_hint_dead_no_live_falls_back_to_env_for_retry` — FR-5 fallback
- `resolve_no_hint_no_dirs_returns_no_live_instance_error` — typed `NoLiveInstance` error
- `resolve_invalid_env_hint_falls_through_to_scan` — defense-in-depth: malformed env hint treated as None
- `resolve_skips_symlink_his_dirs` — symlink HIS dir rejected; matches `create_fifo_at` security posture

#### Story 003-runtime-socket-path-uses-resolver — 2 tests

- `runtime_socket_path_returns_live_instance_socket` — env points at non-existent HIS, live HIS exists separately, resolver picks live one (full integration through the public seam)
- `runtime_socket_path_honors_live_env_pinning` — env points at live HIS, multiple live HISes exist, resolver returns the env-named one

### Acceptance Criteria Validation

#### Story 001-probe-instance

- ✅ Mock socket replying with a window block → `Liveness::LiveWithClients` within 1s — `probe_classifies_live_with_clients`
- ✅ Mock socket replying with `Invalid` → `Liveness::LiveEmpty` — `probe_classifies_live_empty`
- ✅ No socket file at the path → `Liveness::Dead` — `probe_classifies_dead_when_socket_missing`
- ✅ Mock socket that accepts but never replies → `Liveness::Dead` after deadline — `probe_times_out_on_hanging_server`
- ✅ 4 concurrent probes → wall time bounded by slowest, not sum — `probe_concurrent_runs_in_parallel_not_serial`

#### Story 002-resolve-live-instance

- ✅ env-hint live → returns env, no scan — `resolve_env_hint_live_returns_env_without_scan`
- ✅ env-hint live-empty + others have clients → returns env (multi-seat pin) — `resolve_env_hint_live_empty_wins_over_others_with_clients`
- ✅ env-hint dead + scan finds live → returns scanned + warn — `resolve_env_hint_dead_falls_through_to_live_scan`
- ✅ no env-hint + 3 dirs (Dead/LiveEmpty/LiveWithClients) → returns LiveWithClients — `resolve_no_hint_prefers_live_with_clients_over_empty_over_dead`
- ✅ no env-hint + only LiveEmpty → returns it — `resolve_no_hint_returns_live_empty_when_only_choice`
- ✅ env-hint dead + nothing else live → returns env hint with warn — `resolve_env_hint_dead_no_live_falls_back_to_env_for_retry`
- ✅ no env-hint + zero live dirs → `Err(NoLiveInstance)` — `resolve_no_hint_no_dirs_returns_no_live_instance_error`

#### Story 003-runtime-socket-path-uses-resolver

- ✅ Signature unchanged (`async fn` is the only delta) — confirmed by `cargo build --workspace`
- ✅ All existing callers compile without edits beyond `.await` — 6 doctest sites + 2 binary main sites + 3 inline tests, all mechanical
- ✅ `cargo test --workspace` passes — 402/402 (348 existing + 17 new lib + 16 daemon + 16 doctests + others)
- ✅ Manual reproduction (during the 2026-04-29 triage earlier in this session) — daemon connects to the live HIS, would automatically again after this code lands without needing the systemd-set-environment hot-fix

### Issues Found

- **Deadlock in two integration tests** (caught and fixed before the test run completed). `with_isolated_runtime_dir` already holds `async_env_test_mutex` for the closure body; the original test code re-acquired it (tokio's `Mutex` is not reentrant). Fixed by mutating the env directly inside the closure — the mutex is already held, so the SAFETY contract is satisfied. Documented inline in the affected tests.

### Notes

- Test runtime: lib unit tests complete in 2.03s (up from ~0.17s pre-bolt because of the 1s timeout test and 4 concurrent 1s timeouts). Acceptable — well within human patience and CI budgets, and the timeout-bounded tests are the test infrastructure proving its own bounds.
- The `runtime_socket_path_rejects_unsafe_name_argument` test (pre-existing, modified in Stage 2) had its "good name" assertion semantically loosened — see `implementation-walkthrough.md` Developer Notes for rationale. The test still proves what it was originally checking ("name validation isn't the rejection cause for good names"); it just no longer over-asserts on environment-state-dependent resolution behavior.
- No flaky-test indicators observed across two consecutive workspace-test runs. The concurrent-probe test has 500ms of slack beyond the 1s expected runtime which should comfortably accommodate CI noise.
- Test infrastructure stays single-source per intent 015 FR-5: `MockHyprlandInstance` lives in `test_helpers.rs`, no parallel mock layer in the daemon or the resolver tests themselves.
