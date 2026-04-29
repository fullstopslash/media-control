---
id: 002-resolve-live-instance
unit: 001-his-resolve-with-probe
intent: 017-daemon-his-autodetect
status: ready
priority: must
created: 2026-04-29T08:10:00Z
assigned_bolt: 028-his-resolve-with-probe
implemented: false
---

# Story: 002-resolve-live-instance

## User Story

**As a** caller of `runtime_socket_path()` (CLI command or daemon)
**I want** a `resolve_live_his(env_hint) -> Result<String, _>` function that returns the HIS string of the best-available Hyprland instance, honoring an explicit env-var hint when it points to something live and falling back gracefully when nothing probes alive
**So that** I get correct behavior in all five cases the FRs cover (env-set-and-live, env-set-but-stale, env-unset-multi-instance, env-unset-single-instance, no-live-instances) without each caller reimplementing the precedence rules

## Acceptance Criteria

- [ ] **Given** `env_hint = Some("A")` and HIS `A` probes `LiveWithClients`, **When** I call `resolve_live_his(env_hint)`, **Then** it returns `"A"` *without* probing other dirs (FR-2 fast-path)
- [ ] **Given** `env_hint = Some("A")` and HIS `A` probes `LiveEmpty`, while HIS `B` (also present) probes `LiveWithClients`, **When** I call `resolve_live_his(env_hint)`, **Then** it returns `"A"` (env hint wins when alive at all, even if not the "best") and emits a `debug!` line noting the trade-off — multi-seat users explicitly chose A
- [ ] **Given** `env_hint = Some("A")` and HIS `A` probes `Dead`, while HIS `B` probes `LiveWithClients`, **When** I call `resolve_live_his(env_hint)`, **Then** it returns `"B"` and emits a `warn!` line naming `A` as stale (FR-3)
- [ ] **Given** `env_hint = None` and three HIS dirs probe `Dead` / `LiveEmpty` / `LiveWithClients`, **When** I call `resolve_live_his(env_hint)`, **Then** it returns the `LiveWithClients` HIS (FR-1 preference)
- [ ] **Given** `env_hint = None` and the only HIS dir probes `LiveEmpty`, **When** I call `resolve_live_his(env_hint)`, **Then** it returns that HIS (live-empty is acceptable when nothing better exists)
- [ ] **Given** `env_hint = Some("A")` and `A` probes `Dead` and no other HIS dirs probe alive, **When** I call `resolve_live_his(env_hint)`, **Then** it returns `"A"` with a `warn!` line — let the caller's existing retry loop deal with it (FR-5)
- [ ] **Given** `env_hint = None` and zero HIS dirs exist (or all are `Dead`), **When** I call `resolve_live_his(env_hint)`, **Then** it returns `Err(HyprlandError::…)` with a typed variant naming "no Hyprland instance reachable"

## Technical Notes

- Probe candidates concurrently using `tokio::task::JoinSet` — each `probe_instance` call has its own 1s deadline so the overall resolve completes within ~1.1s worst case regardless of instance count
- Precedence summary (apply in order, first match wins):
  1. `env_hint` set AND probes alive (`LiveWithClients` OR `LiveEmpty`) → return `env_hint`
  2. `env_hint` set AND probes `Dead` → `warn!` naming the stale HIS, fall through
  3. Among scanned dirs, return the first `LiveWithClients` (mtime-sorted dirs, newest first, as tiebreaker)
  4. If no `LiveWithClients`, return the first `LiveEmpty` (mtime-sorted)
  5. If no live anything: return `env_hint` if Some (so the caller's retry loop has a target), else first scanned dir, else `Err`
- Reuse `env::var("HYPRLAND_INSTANCE_SIGNATURE")` lookup in `runtime_socket_path` (next story); this function takes the hint as a parameter so it's testable without env mutation
- Add a typed `HyprlandError` variant for the no-live case (don't reuse the existing socket-connect error variant — different failure mode, different log line)

## Dependencies

### Requires

- 001-probe-instance (must exist before this story can call it)

### Enables

- 003-runtime-socket-path-uses-resolver (calls this directly)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `$XDG_RUNTIME_DIR` is unset | Return `Err(HyprlandError::…)` matching today's behavior — don't paper over a broken environment |
| `$XDG_RUNTIME_DIR/hypr/` doesn't exist (Hyprland never started this session) | Return `env_hint` if set (let backoff loop wait for Hyprland), else `Err` |
| HIS dir name is invalid UTF-8 | Skip silently with a `debug!` log; continue scanning. Don't fail the whole resolve over one bad entry |
| Two HIS dirs both probe `LiveWithClients` (multi-seat real-world case) | Return the most-recently-modified one (mtime tiebreaker) — and `info!` log the choice so multi-seat users can debug |
| `env_hint` is set to an HIS string that has no corresponding dir | Treat as `Dead`; falls through to scanning |

## Out of Scope

- Per-call caching of probe results
- Reading user preference for tiebreaker order from config (defer until someone actually asks for it)
- Validating the HIS string format (assume Hyprland-generated; pass-through)
