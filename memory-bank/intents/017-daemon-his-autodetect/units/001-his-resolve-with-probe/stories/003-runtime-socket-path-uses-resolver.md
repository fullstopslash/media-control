---
id: 003-runtime-socket-path-uses-resolver
unit: 001-his-resolve-with-probe
intent: 017-daemon-his-autodetect
status: ready
priority: must
created: 2026-04-29T08:10:00Z
assigned_bolt: 028-his-resolve-with-probe
implemented: false
---

# Story: 003-runtime-socket-path-uses-resolver

## User Story

**As a** caller of `media_control_lib::hyprland::runtime_socket_path(basename)` (every CLI command and the daemon)
**I want** the function to internally resolve the *live* HIS instead of reading `HYPRLAND_INSTANCE_SIGNATURE` blindly, with no signature change
**So that** the entire fix lands without touching any callsite — CLI commands and the daemon both pick up correct behavior with one substrate change

## Acceptance Criteria

- [ ] **Given** the existing `runtime_socket_path(basename: &str) -> Result<PathBuf, HyprlandError>` signature, **When** the body is replaced to call `resolve_live_his(env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().as_deref())` and join with `basename`, **Then** all existing callers (`get_socket2_path` in daemon, every `commands::*` module that opens a socket) compile and test unchanged
- [ ] **Given** the 2026-04-29 incident reproduction (env var points at a `LiveEmpty` HIS while a `LiveWithClients` HIS exists in another dir), **When** I run a CLI command (e.g., `media-control fullscreen`) AND start the daemon, **Then** both connect to the live-with-clients instance, and the daemon journal includes a `warn!` line naming the stale env HIS
- [ ] **Given** a single-instance host (the common case), **When** I run any CLI command or start the daemon, **Then** behavior is observably identical to today (the env-fast-path in `resolve_live_his` returns immediately on a live env hint with no scan)
- [ ] **Given** existing CLI integration tests in `crates/media-control-lib/tests/`, **When** the change lands, **Then** `cargo test --workspace` passes with no test edits required (or with edits that strictly tighten assertions, not loosen them)
- [ ] **Given** the daemon is restarted with the corrected systemd env (the hot-fix from 2026-04-29), **When** I now restart with the *broken* env again, **Then** the daemon still connects to the live instance (proves the fix is durable, not just dependent on the hot-fix)

## Technical Notes

- The body change is small — roughly: read env var, call resolver, build path. The complexity is in the call patterns the resolver enables, not the wrapper itself
- The `warn!` line in `resolve_live_his` (story 002) emits when the env hint is stale. That's where the stale-env signal surfaces in logs, so this story doesn't need its own logging
- Verify no caller of `runtime_socket_path` cached its result across long lifetimes (a stale-cached path would defeat the fix). Grep callsites: today they're `get_socket2_path()` in daemon (called fresh on each `connect_hyprland_socket` call) and `_hypr_cmd`-equivalent paths in commands (called per-invocation). All look fine, but verify in the bolt's design stage
- The `XDG_RUNTIME_DIR` env-mutex pattern in `test_helpers.rs` (per intent 015) is needed for any test that asserts on resolution behavior since the resolver reads env state

## Dependencies

### Requires

- 002-resolve-live-instance (the wrapper calls it)

### Enables

- 002-001-connect-loop-re-resolves (Unit 2 — the per-iteration resolve in the daemon's connect loop relies on `runtime_socket_path()` going through the new resolver)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `HYPRLAND_INSTANCE_SIGNATURE` is unset | Pass `None` to resolver; resolver scans dirs and picks best |
| `HYPRLAND_INSTANCE_SIGNATURE` is set to empty string | Treat as unset (Some("") → None) — defends against bash setting an empty var |
| Resolver returns Err (no Hyprland reachable) | Propagate as today's error variant via `?` so existing retry/backoff loops are triggered |
| Probe overhead noticeable on CLI cold start | Acceptable per FR-1 budget (<100ms with 4 instances; single-instance case is one fast probe) |

## Out of Scope

- Migrating `runtime_dir()` (different function; not used for HIS path resolution)
- Caching the resolver's result for the duration of a single CLI invocation (probe is cheap; revisit only if measured cost is annoying)
- Adding a `--his` CLI flag to override resolution (not requested; env var already serves this purpose)
