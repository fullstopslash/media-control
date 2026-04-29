---
stage: implement
bolt: 028-his-resolve-with-probe
created: 2026-04-29T08:35:00Z
---

## Implementation Walkthrough: his-resolve-with-probe

### Summary

Added probe-based Hyprland-instance-signature (HIS) resolution to `media-control-lib::hyprland`. `runtime_socket_path()` now scans `$XDG_RUNTIME_DIR/hypr/*/`, picks a live instance (preferring the one named by `HYPRLAND_INSTANCE_SIGNATURE` when alive), and falls through to autodetection when the env hint points at a stale instance. The change deliberately propagates async-ness up through `HyprlandClient::new()` and `CommandContext::with_config()` to keep the resolver's tokio I/O honest — see *Deviations from Plan* for why we abandoned the sync-`block_on` Option A.

### Structure Overview

The resolver lives entirely in `hyprland.rs`, alongside its existing per-instance helpers. Three new `pub(crate)` items (`Liveness`, `probe_instance`, `resolve_live_his`) plus one new `pub` enum variant (`HyprlandError::NoLiveInstance`). `runtime_socket_path()`'s public signature is now `async fn` but otherwise unchanged in name and parameters. Two small extracted helpers (`validated_runtime_dir`, `is_safe_his`) factor out the security checks the original inline body performed.

The downstream callsite touch list is small and mechanical: every caller of the now-async `runtime_socket_path` / `HyprlandClient::new` / `CommandContext::with_config` adds `.await`. All callers were already inside an async context (`#[tokio::main]` for both binaries; `#[tokio::test]` for the unit tests), so no caller had to be lifted to async — only `.await` was added.

### Completed Work

- [x] `crates/media-control-lib/src/hyprland.rs` — added `Liveness` enum, `probe_instance`, `resolve_live_his`, `validated_runtime_dir`, `is_safe_his`; replaced `runtime_socket_path` body and made it `async`; made `HyprlandClient::new` `async`; updated 5 doctest sites for the new `.await`
- [x] `crates/media-control-lib/src/error.rs` — added `From<HyprlandError>` arm for the new `NoLiveInstance` variant (maps to `SocketNotFound` so the daemon's existing not-found backoff path applies)
- [x] `crates/media-control-lib/src/commands/shared.rs` — `CommandContext::new()` and `CommandContext::with_config()` are now `async`
- [x] `crates/media-control-lib/src/commands/mod.rs`, `commands/window/{close,focus,move_window}.rs` — module- and function-level doctests updated for new `.await`
- [x] `crates/media-control-daemon/src/main.rs` — `get_socket2_path()` is now `async`; `connect_hyprland_socket()` resolves the path **inside** its retry loop (lays the foundation for bolt 029's tighter inner-loop work, and already gives the FR-4 outer-loop behavior for free)
- [x] `crates/media-control/src/main.rs` — single `.await` added to the `CommandContext::with_config` call
- [x] `crates/media-control-lib/src/commands/window/mod.rs` — three pre-existing tests updated: two added `.await` to the now-async `runtime_socket_path`; one (`runtime_socket_path_rejects_unsafe_name_argument`) had its "good name" assertion loosened to "name is not the rejection cause" since resolution may now return Ok-or-non-name-Err depending on the test environment's `/tmp/hypr/` state — semantically equivalent for what that test is actually checking

### Key Decisions

- **`runtime_socket_path` made async (deviated from plan's Option A)**: see *Deviations from Plan* below
- **`probe_instance` returns `Liveness` not `Result<Liveness, _>`**: any failure mode (refused, missing, perm-denied, timeout) is semantically the same to the resolver — "this instance is not usable." Returning `Result` would force every caller to map error→Dead, doubling boilerplate without information gain
- **Newest-mtime tiebreaker**: if multiple instances probe `LiveWithClients`, the newest HIS dir wins. Hyprland creates the dir at instance startup, so newer = more recent session. Practical heuristic for multi-seat hosts; trivial to override with `HYPRLAND_INSTANCE_SIGNATURE` per FR-2
- **Symlinks rejected**: matches the daemon's `create_fifo_at` security posture. A symlinked HIS dir at `$XDG_RUNTIME_DIR/hypr/` is suspicious — Hyprland doesn't create them; if one appears, treat as hostile and skip. No CVE in the wild for this, just defense in depth
- **`probe_instance` reads up to 8 KiB then stops**: `activewindow` replies are typically < 200 bytes (one window block); 8 KiB is generous against any growth and prevents a hostile peer from streaming megabytes
- **Defense-in-depth re-validation in `runtime_socket_path`**: even though `resolve_live_his` filters HIS strings via `is_safe_his`, `runtime_socket_path` re-validates the returned HIS before using it in path construction. A future refactor that loosens the resolver's filter cannot accidentally re-introduce path traversal at the construction site
- **`Liveness` and friends kept `pub(crate)`**: `runtime_socket_path` is the public seam; downstream code never needs to inspect liveness directly. Smaller API surface = fewer breaking-change concerns for future revisions

### Deviations from Plan

**Switched from Option A (sync `runtime_socket_path` with internal `block_on`) to Option B (async all the way).**

The plan's Option A would have called `tokio::runtime::Builder::new_current_thread().build().block_on(...)` from inside `runtime_socket_path`. This panics with "Cannot start a runtime from within a runtime" because every actual caller (`#[tokio::main]` daemon, `#[tokio::main]` CLI, `#[tokio::test]` tests) is already inside a tokio runtime when it reaches `runtime_socket_path`.

Workarounds existed (separate `std::thread::spawn` with its own runtime; `tokio::task::block_in_place` requiring multi-threaded scheduler), but each adds fragility for no real benefit since every caller was already in async context. Option B's diff turned out to be ~5 callsite changes (each adding `.await`) plus some doctest updates — net cleaner than the workarounds.

This was flagged to the user mid-Stage-2 before implementing; they signed off implicitly via the existing approval flow.

**Why the deviation matters for future bolts**: Bolt 029 (daemon-reconnect-re-resolution) is now slightly redundant with what landed here. The re-resolve-per-iteration of the daemon's connect loop already happens because `connect_hyprland_socket`'s loop body now calls `get_socket2_path().await`, which calls `runtime_socket_path().await`, which re-resolves. The bolt is still valuable as an explicit test case (the swap-mid-retry scenario) and to verify the FR-4 acceptance criterion in isolation, but the production code change for FR-4 is in fact already in place.

### Dependencies Added

- [x] No new runtime crates (as planned)
- [x] No new dev-dependencies needed for Stage 2's code (Stage 3's tests will reuse `tempfile` which is already a dev-dep)

### Developer Notes

- The probe sends `activewindow` (no newline). Hyprland's IPC protocol is request-then-shutdown-write-half-then-read-EOF; a trailing `\n` is unnecessary and could cause an extra empty line in some clients. The existing `command_inner` in `HyprlandClient` does the same.
- `tokio::task::JoinSet` is used for concurrent probes. It's in the `rt` feature which is already enabled.
- The resolver does NOT cache results across calls. Each `runtime_socket_path` invocation re-probes. Cost: one Unix-socket connect+roundtrip per HIS dir, sub-ms per instance. For CLI commands (one resolve per invocation) this is negligible. For the daemon (one resolve per `connect_hyprland_socket` retry tick) it's also negligible because retries already have a 500ms→10s backoff.
- The `runtime_socket_path_rejects_unsafe_name_argument` test was the only test whose semantics had to change. Its original intent was "name validation rejects bad inputs and accepts good ones." Post-refactor, "accepts good ones" depends on whether resolution succeeds against the real `/tmp/hypr/` (which may or may not contain a live Hyprland during CI). The test now asserts the negative form: "good names are not the *cause* of rejection," which preserves the original intent without depending on environment state.
- All 385 existing tests pass; clippy clean with `-D warnings`. No new tests in this stage — those land in Stage 3 per the simple-construction-bolt template.
