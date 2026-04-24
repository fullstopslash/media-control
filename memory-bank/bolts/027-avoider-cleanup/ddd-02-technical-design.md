---
stage: design
bolt: 027-avoider-cleanup
created: 2026-04-26T00:00:00Z
---

## Technical Design — avoider-cleanup

### Architecture pattern

Two scopes, executed as one bolt:

1. **Intra-`avoid.rs` cleanup** (stories 001-004): apply the audit hit list to `crates/media-control-lib/src/commands/window/avoid.rs`. Pure logic-preserving DRY/clarity passes. Tests stay green throughout.
2. **Daemon-owned hot-path state** (stories 005-006): introduce `ClientCache` (TTL-of-one-debounce) and `SuppressState` (`Arc<AtomicU64>` mirror) in `crates/media-control-daemon/src/main.rs`. Lib's `commands::avoid()` call site in the daemon consults the cache; lib stays stateless.

### Layer Structure

```text
┌────────────────────────────────────────────────────┐
│  media-control-daemon                              │
│   ├── ClientCache (Mutex<Option<(Instant, Vec)>>)  │  ← Story 005
│   ├── SuppressState (Arc<AtomicU64>)               │  ← Story 006
│   └── event loop dispatches commands::avoid(ctx)   │
└────────────────────────────────────────────────────┘
                       │ ctx + cached snapshot
                       ▼
┌────────────────────────────────────────────────────┐
│  media-control-lib::commands::window::avoid        │
│   ├── Rect (geometry newtype)                      │  ← Story 001
│   ├── PositionResolver                             │  ← Story 002
│   ├── AvoidCase (collapsed dispatch)               │  ← Story 003
│   ├── restore_focus_suppressed (helper)            │  ← Story 003
│   └── named constants (FULLSCREEN_NONE etc.)       │  ← Story 003
└────────────────────────────────────────────────────┘
                       │
                       ▼
┌────────────────────────────────────────────────────┐
│  media-control-lib::test_helpers                   │
│   ├── ClientBuilder (builder for synthetic Client) │  ← Story 004
│   └── with_isolated_runtime_dir (env-mutex guard)  │  ← Story 004
└────────────────────────────────────────────────────┘
```

### Mapping audit hit-list items to stories

The audit produced 15 prioritized items. Each maps to a story:

| Audit # | Item | Story | Notes |
|---|---|---|---|
| 1 | Plumb `is_minified()` once per tick | 002 | Thread `minified: bool` through `move_media_window`, `try_move_clear_of`, `handle_move_to_primary`, `handle_fullscreen_nonmedia`, `handle_geometry_overlap` |
| 2 | Two duplicate `overlaps_focused` closures | 001 | Collapsed via `Rect::overlaps` |
| 3 | `get_position_pair` + `calculate_target_position` rebuild closures | 002 | `PositionResolver { ctx, minified }` |
| 4 | `get_clients()` round-trips per event | 005 | TTL-cached in daemon |
| 5 | `media_windows: Vec` allocates each tick | (folded into 005) | Reuse Vec or convert `find_media_windows` to `impl Iterator` |
| 6 | `avoid()` god-function (78 LOC dispatcher) | (optional, in 003) | Extract `prepare_avoid_input` if scope allows; defer otherwise |
| 7 | `classify_case` + `match case` redundant double dispatch | 003 | Inline classification or `AvoidCase::dispatch` |
| 8 | `suppress_avoider()` + `restore_focus()` pair duplicated 2× | 003 | Extract `restore_focus_suppressed` |
| 9 | Magic numbers without constants | 003 | `FULLSCREEN_NONE`, `PERCENT_MAX`, `SCRATCHPAD_MONITOR` |
| 10 | `should_suppress` does file IO every tick | 006 | In-memory `Arc<AtomicU64>` |
| 11 | `FocusedWindow` is a copy of `Client` with bools | 003 | Borrowed view; defer if scope grows |
| 12 | Test scenario builders should move to test_helpers | 004 | `ClientBuilder` |
| 13 | `assert_handler_warms_suppression` env-mutex dance | 004 | `with_isolated_runtime_dir` |
| 14 | `tracing::debug!` repeated per arm | 003 | One `Display`-driven log on `AvoidCase` |
| 15 | `rectangles_overlap` 8-arg signature | 001 | Replaced by `Rect::overlaps` |

### API Design

**Public surface** (no breaking changes):

- `Rect` is `pub(crate)` in `commands/window/mod.rs` (or a new private `commands::window::geometry` module). External consumers don't see it; `avoid()` and other window commands use it internally.
- `PositionResolver` is `pub(crate)` in the same scope. Same reasoning.
- `restore_focus_suppressed` is `pub(crate)` in `commands::window::mod`.
- `AvoidCase` stays private to `avoid.rs` (no external consumers today).
- `ClientBuilder` is `pub` in `crate::test_helpers` (only used by tests, but tests are external to the modules they test).
- `with_isolated_runtime_dir` is `pub` in `crate::test_helpers`.

**Daemon's new state** is private to the daemon's `main.rs` — not exposed across the crate boundary.

### Data Model

| Type | Definition |
|---|---|
| `Rect` | `struct Rect { x: i32, y: i32, w: i32, h: i32 }`; `Copy + Clone + Debug + PartialEq` |
| `PositionResolver<'a>` | `struct PositionResolver<'a> { ctx: &'a CommandContext, minified: bool }` |
| `ClientCache` | `Mutex<Option<(Instant, Vec<Client>)>>` — initially `None` |
| `SuppressState` | `Arc<AtomicU64>` — initially `0` (treated as "no recent suppress") |
| `FocusedWindow<'a>` | `struct FocusedWindow<'a> { client: &'a Client, is_media: bool }` (refactor of existing by-value type) |

### Security design

No new attack surface. The cache holds the same data the avoider already reads from Hyprland; the in-memory suppress state mirrors a file the daemon already wrote. Both are local-process-only and survive only for the daemon's lifetime.

### NFR implementation

| NFR | Approach |
|---|---|
| Per-event syscall count strictly lower (FR-3 NFR) | Stories 002 (one `is_minified()` per tick) + 006 (skip suppress-file stat) collectively eliminate redundant syscalls |
| Per-event allocation count strictly lower (FR-3 NFR) | Story 005 (cached Vec<Client>) + audit item 5 (reused/iterator find_media_windows) |
| `avoid.rs` production code strictly smaller (FR-3 NFR) | Stories 001-003 collectively (Rect collapses two closures + 8-arg fn; PositionResolver collapses repeated closure builds; classify-dispatch collapse) |
| Behavioral parity (FR-3 acceptance) | Existing tests must pass without modification beyond mechanical helper-extraction in story 004 |
| Test infra single-source (FR-5) | Story 004; daemon's new state tests must use lib's `test_helpers`, not grow a parallel mock |

### Plan integrations

- **`commands::shared::async_env_test_mutex`**: bolt 025 left this in `commands/shared.rs`; story 004 will likely move it to `test_helpers.rs` alongside `with_isolated_runtime_dir` (the env-mutex dance has to live where the env-mutex itself lives). Acceptable if it stays in `shared.rs` — `with_isolated_runtime_dir` can re-export or call into it. Decided at implementation: whichever produces less churn.
- **Daemon's existing event-loop structure**: cache and suppress state are added to the daemon's main loop without restructuring it. No new trait abstractions; just two `Mutex`/`Arc` fields owned by the loop's outer scope and read inside the per-event handler.

### Implementation footprint (preview of Stage 4)

Files modified:

- `crates/media-control-lib/src/commands/window/avoid.rs` — bulk of the cleanup; production code shrinks; tests update mechanically
- `crates/media-control-lib/src/commands/window/mod.rs` (or a new `commands/window/geometry.rs`) — `Rect`, `PositionResolver`, `restore_focus_suppressed`
- `crates/media-control-lib/src/test_helpers.rs` — `ClientBuilder`, `with_isolated_runtime_dir`
- `crates/media-control-daemon/src/main.rs` — `ClientCache`, `SuppressState`, integration into the event loop

Files NOT touched:

- `commands/workflow/*` — out of scope (workflow is unrelated to avoidance)
- `jellyfin.rs` — out of scope
- `commands/shared.rs` — possibly `async_env_test_mutex` migrates out (open during impl)
- `Cargo.toml` files — no new deps; the audit explicitly noted `arc-swap` only if benchmarks justify, and we have no benchmarks budget here

### Risks

- **Story 004 (test scenario migration)** churns many test files. Risk: import-path break. Mitigation: rust-expert iterates with `cargo test` between each migrated builder.
- **Story 005 (ClientCache)** introduces a `Mutex` in the daemon's hot path. Risk: lock contention if events fire in bursts faster than the lock can serialize. Mitigation: 15 ms TTL means the lock is held briefly; if benchmarks ever show contention, switch to `arc_swap` in a future bolt.
- **Story 006 (in-memory suppress)** changes the daemon's behavior subtly: daemon's own `suppress_avoider` calls no longer write the file, only update memory. Risk: a third-party tool reading the file misses daemon-internal warmings. Verified at design time: no other reader of the suppress file exists outside the daemon's own check (`should_suppress`). Mitigation: the daemon's reads still consult the file as fallback, so cross-process callers (CLI commands warming the daemon) remain authoritative for *their* writes.
- **Scope creep**: the audit's optional item 6 (extract `prepare_avoid_input`) and item 11 (FocusedWindow refactor) are both flagged as "do if scope allows; defer otherwise." The implementation agent decides at execution time based on diff size.

### What this bolt deliberately does NOT do

- Re-architect the avoidance algorithm (algorithm is correct; cleanup only)
- Add new avoidance cases (5 cases is the current set; this bolt preserves them)
- Replace `Mutex` with lock-free primitives (defer until benchmarks justify)
- Migrate `async_env_test_mutex` to a different concurrency primitive (out of scope)
- Touch the FIFO trigger or signal-handling code in the daemon (separate concern)
- Address the pre-existing flake in `move_down_dispatches_correct_position` (could be folded into story 004's env-mutex work if the implementer sees an obvious fix; otherwise documented and deferred)
