---
stage: plan
bolt: 028-his-resolve-with-probe
created: 2026-04-29T08:20:00Z
---

## Implementation Plan: his-resolve-with-probe

### Objective

Replace the env-var-only HIS lookup in `media-control-lib::hyprland::runtime_socket_path()` with a probe-based resolver that picks a *live* Hyprland instance, honors `HYPRLAND_INSTANCE_SIGNATURE` when its target is alive, and warns when the env points at a stale instance. CLI commands and the daemon both benefit through one substrate change because both already route through `runtime_socket_path()`.

### Deliverables

- New `Liveness` enum in `crates/media-control-lib/src/hyprland.rs` with three variants: `LiveWithClients`, `LiveEmpty`, `Dead`
- New async function `probe_instance(his: &str) -> Liveness` (1s timeout, classifies via `activewindow` reply)
- New async function `resolve_live_his(env_hint: Option<&str>) -> Result<String, HyprlandError>` implementing the precedence ladder
- New `HyprlandError::NoLiveInstance` variant for the "scan found nothing AND no env hint" case
- `runtime_socket_path(name)` body replaced to call `resolve_live_his()` (signature unchanged)
- New `MockHyprlandInstance` builder in `crates/media-control-lib/src/test_helpers.rs` with response policies `LiveWithClients`, `LiveEmpty`, `Hang`, `Refuse`
- 11+ new unit tests covering the FR-1/2/3/5 acceptance matrix from the stories

### Dependencies

- **`tokio::time::timeout`**: bounds each probe to 1s — already in dep tree
- **`tokio::task::JoinSet`**: concurrent probes across HIS dirs — already in dep tree (tokio "rt" feature)
- **`tokio::net::UnixListener`**: mock-server side of the probe tests — already in dep tree (tokio "net")
- **`tempfile`**: per-test `XDG_RUNTIME_DIR` isolation — already a dev-dep
- No new runtime crates

### Technical Approach

#### File-level changes

```text
crates/media-control-lib/src/hyprland.rs   ← grow by ~150 LOC
  + Liveness enum (pub(crate))
  + probe_instance() (pub(crate))
  + resolve_live_his() (pub(crate))
  + HyprlandError::NoLiveInstance variant (pub)
  ~ runtime_socket_path() body replaced (pub, signature unchanged)

crates/media-control-lib/src/test_helpers.rs   ← grow by ~120 LOC
  + MockHyprlandInstance builder + ResponsePolicy enum
  + with_isolated_runtime_dir helper (if not already present in some form)
  + spawn_mock_socket_server() task helper
```

#### Resolver precedence ladder (the heart of the change)

`resolve_live_his(env_hint)` in pseudocode:

```text
1. If env_hint = Some(h):
     match probe_instance(h):
       LiveWithClients | LiveEmpty   → return h          (FR-2 fast path)
       Dead                          → warn!("stale env"), fall through

2. Scan $XDG_RUNTIME_DIR/hypr/*/  (skip env_hint dir if already probed Dead)
   Probe all candidates concurrently via JoinSet (each with 1s timeout).
   Collect (his, liveness, dir_mtime) tuples.

3. Pick best candidate:
     a. Any LiveWithClients?  → newest by mtime         (FR-1 preferred)
     b. Any LiveEmpty?         → newest by mtime         (FR-1 fallback)

4. Nothing live found:
     If env_hint = Some(h)  → return h     (FR-5: let caller's retry loop deal)
     Else if any dir exists → return first dir (mtime-newest)
     Else                   → Err(HyprlandError::NoLiveInstance)
```

Concurrency: all probes run via `JoinSet`, so total wall time is bounded by the slowest single probe (~1s worst case, much faster on healthy systems).

#### Liveness classification (probe_instance)

```text
1. Connect: tokio::net::UnixStream::connect($XDG_RUNTIME_DIR/hypr/{his}/.socket.sock)
   - Refused / not-found / not-a-socket → Liveness::Dead

2. Inside tokio::time::timeout(1s):
     Write "activewindow\n"
     Read one line via BufReader::read_line()
     - "Invalid" (trimmed) → Liveness::LiveEmpty
     - empty bytes / EOF before data → Liveness::LiveEmpty (server closed without payload)
     - any other content → Liveness::LiveWithClients
   - Timeout → Liveness::Dead
```

Symlink HIS dirs are rejected at the symlink-metadata check (matches the `create_fifo_at` security posture in the daemon). Permission-denied opening the socket → `Dead` with `debug!`-level log (multi-user containers are legitimate).

#### `runtime_socket_path` after the change

```text
pub fn runtime_socket_path(name: &str) -> Result<PathBuf> {
    let env_hint_owned = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
    let env_hint = env_hint_owned.as_deref().filter(|s| !s.is_empty());
    // resolve_live_his is async; this fn stays sync — block_on a tiny runtime
    // OR convert callers to async (deferred — keep signature).
    let his = block_on(resolve_live_his(env_hint))?;
    Ok(runtime_dir_for(his)?.join(name))
}
```

**Open design question** for the implementation stage: `runtime_socket_path` is currently sync. Two choices:

- **A.** Keep it sync; use a tiny `tokio::runtime::Builder::new_current_thread()` to block on the resolver. Adds a small one-shot runtime spin per CLI invocation (~ms cost).
- **B.** Make `runtime_socket_path` async. Touches every caller site (CLI commands, daemon). Cleaner long-term but expands the diff.

**Recommend A** for this bolt — keeps the diff surgical. If we hit measurable cost in practice, switch to B in a follow-up.

#### Why `pub(crate)` for `Liveness` / `probe_instance` / `resolve_live_his`

`runtime_socket_path` is the public seam. Exposing the new types broadens the lib's API surface for no benefit — daemon and CLI never need them directly. Keep crate-private; revisit only if a real caller appears.

### Acceptance Criteria (rolled up from stories 001/002/003)

#### Story 001-probe-instance

- [ ] Mock socket replying with a window block → `Liveness::LiveWithClients` within 1s
- [ ] Mock socket replying with `Invalid\n` → `Liveness::LiveEmpty`
- [ ] No socket file at the path → `Liveness::Dead`
- [ ] Mock socket that accepts but never replies → `Liveness::Dead` after 1s deadline
- [ ] 4 concurrent probes → wall time bounded by slowest, not sum

#### Story 002-resolve-live-instance

- [ ] `env_hint` set + that HIS probes `LiveWithClients` → returns env hint, no scan
- [ ] `env_hint` set + that HIS probes `LiveEmpty`, others have `LiveWithClients` → returns env hint (explicit choice wins) + `debug!` log
- [ ] `env_hint` set + that HIS probes `Dead`, scan finds `LiveWithClients` → returns scanned + `warn!` naming stale env
- [ ] `env_hint = None` + 3 dirs (Dead/LiveEmpty/LiveWithClients) → returns `LiveWithClients`
- [ ] `env_hint = None` + only `LiveEmpty` exists → returns it
- [ ] `env_hint = Some(stale)` + nothing else live → returns env hint with `warn!` (FR-5 caller-handles)
- [ ] `env_hint = None` + zero live dirs (or zero dirs) → `Err(NoLiveInstance)`

#### Story 003-runtime-socket-path-uses-resolver

- [ ] `runtime_socket_path` signature unchanged (`fn(&str) -> Result<PathBuf>`)
- [ ] All existing callers compile without edits
- [ ] `cargo test --workspace` passes (existing tests unmodified)
- [ ] Manual: reproduce 2026-04-29 incident — daemon and CLI both connect to live instance regardless of env value

### Implementation Order (for Stage 2)

1. **Test scaffolding first**: `MockHyprlandInstance` + helpers in `test_helpers.rs`. Failing tests for `probe_instance` written upfront (TDD).
2. **`probe_instance`**: enough code to pass the probe-level tests.
3. **Failing tests for `resolve_live_his`** (the 7-case matrix).
4. **`resolve_live_his`**: precedence ladder + concurrent probe; pass tests.
5. **`HyprlandError::NoLiveInstance`** variant.
6. **`runtime_socket_path` body swap**.
7. **Verify `cargo test --workspace` clean** + `cargo clippy --workspace -- -D warnings` clean.

### Risks & Open Questions

| Risk | Mitigation |
|------|------------|
| Probe deadlock on a wedged Hyprland | 1s `tokio::time::timeout` per probe — hard ceiling |
| `runtime_socket_path` is called from sync code (config.rs init paths?) | Survey at start of Stage 2; if found, use Option A (block_on). Verified sync today: only `get_socket2_path` in daemon and similar paths in commands — all callable from async context anyway, but keep sync wrapper for compat |
| Hyprland's `Invalid` reply format changes between versions | Treat ANY non-empty non-`Invalid` reply as `LiveWithClients`; degrades gracefully (slightly worse picks but no failure) |
| CLI invocation startup cost grows visibly | Single-instance fast path: env hint probe is one connect+roundtrip on a Unix socket (sub-ms). Negligible. |
