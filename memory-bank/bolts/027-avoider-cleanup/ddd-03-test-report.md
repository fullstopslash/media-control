---
stage: test
bolt: 027-avoider-cleanup
created: 2026-04-26T00:00:00Z
---

## Test Report — avoider-cleanup

### Summary

- **Workspace tests** (`cargo test --workspace -- --test-threads=1`): **385/385 pass** (was 372 in bolt 026; +13 from this bolt)
- **Lint** (`cargo clippy --workspace --all-targets -- -D warnings`): clean
- **Lint, daemon-only** (`cargo clippy -p media-control-daemon --all-targets -- -D warnings`): clean
- **Daemon single-package build** (`cargo build -p media-control-daemon`, cli off): clean — no workflow code leaked into the daemon's lib build
- **Boundary test from bolt 026**: still passes (the new daemon state didn't introduce forbidden imports)
- **`avoid.rs` total LOC**: 2234 → **1894** (−340; production region roughly stable, test churn collapsed)

### Test Suite Breakdown

| Suite | Pass | Fail | Δ from bolt 026 | Notes |
|---|---|---|---|---|
| `media-control-lib` lib unit | 348 | 0 | +5 | New `Rect`/geometry tests |
| `media-control-daemon` binary unit | 16 | 0 | +8 | 4× `ClientCache` + 4× `SuppressState` |
| `media-control-daemon` `boundary` integration | 2 | 0 | 0 | Bolt 026's grep test, unchanged |
| `media-control` binary unit | 0 | 0 | 0 | None defined |
| `config_integration` integration | 3 | 0 | 0 | Untouched |
| Doc-tests | 16 | 0 | 0 | Untouched |
| **Total** | **385** | **0** | **+13** | |

### Acceptance Criteria Validation (against unit brief 003 and the 6 stories)

#### Story 001 — Rect newtype + overlap helpers ✅

- ✅ `Rect { x, y, w, h }` with `Copy + Clone + Debug + PartialEq` introduced in `commands/window/geometry.rs`
- ✅ `Rect::overlaps` preserves the `i64` widening (overflow tests at avoid.rs equivalents pass without modification)
- ✅ 8-arg `rectangles_overlap` removed from production; kept as `#[cfg(test)]` shim purely for the overflow test suite (clean — old test bodies didn't have to change)
- ✅ Two duplicate `overlaps_focused` closures + the `overlaps_peer` closure unified via `Rect::overlaps`
- ✅ Plus: `FocusedWindow::rect()` accessor added — bonus DRY win

#### Story 002 — Plumb minified once + PositionResolver ✅

- ✅ `is_minified()` resolved exactly once per `avoid_with_clients` call
- ✅ `minified: bool` threaded through all 5 handlers + `move_media_window`, `try_move_clear_of`, `calculate_target_position`, `get_position_pair`
- ✅ `PositionResolver<'a> { ctx, minified }` with `resolve_or` / `resolve_opt` methods; collapses the 4 open-coded resolves in both `get_position_pair` and `calculate_target_position`

#### Story 003 — Collapse classify-dispatch + restore_focus_suppressed + named constants ✅ (audit item 11 deferred)

- ✅ `classify_case` became `AvoidCase::classify`; `AvoidCase::dispatch` collapses post-classify match in `avoid_with_clients` to one method call
- ✅ `Display for AvoidCase` replaces 5 per-arm `tracing::debug!` lines with one
- ✅ `restore_focus_suppressed` extracted; both former duplicate sites collapsed to one line each
- ✅ Named constants `FULLSCREEN_NONE`, `PERCENT_MAX`, `SCRATCHPAD_MONITOR` introduced with doc-comments naming the regression-test backstop where applicable
- ⚠️ **Audit item 11 (`FocusedWindow` → `&Client` view) deferred** — the existing struct already only carries cheap copies (`&str` references + a few i32s); converting to a borrowed view would shift work to per-field accessor calls and add lifetime noise to all 4 handler signatures for ~20 LOC win. Cost-benefit didn't favor it. Documented in implementation report; can be revisited in a future cleanup if the tradeoff shifts.

#### Story 004 — Migrate scenario builders to test_helpers ✅

- ✅ `ClientBuilder` (chainable) introduced in `test_helpers.rs`; ~30 `make_test_client_full(...)` call sites in `avoid.rs` migrated
- ✅ `scenario_single_workspace_json` migrated to `test_helpers.rs`
- ✅ `with_isolated_runtime_dir<F, Fut, R>` primitive with RAII `RuntimeDirGuard` — restores `XDG_RUNTIME_DIR` on every exit path including panic
- ✅ `async_env_test_mutex` stayed in `commands/shared.rs` (less churn) and `with_isolated_runtime_dir` calls into it
- ✅ FR-5 satisfied: NO `mod test_helpers` in the daemon. Daemon tests use public types directly (Client/Workspace literals) for cache-state tests; no parallel mock infrastructure.

#### Story 005 — Daemon ClientCache, TTL-of-one-debounce ✅

- ✅ Lib gained `pub async fn avoid_with_clients(ctx, clients: &[Client])`; `avoid()` delegates after fetching — preserves the existing public API
- ✅ Daemon `ClientCache: Mutex<Option<(Instant, Vec<Client>)>>` with `get_or_refresh` factored into `try_hit` + `install` for testability
- ✅ TTL = `Duration::from_millis(u64::from(debounce_ms))` from `config.positioning.debounce_ms` — reuses the existing config value (no new tuning knob)
- ✅ `tracing::debug!` for hit/miss/refetch
- ✅ 4 new tests verify hit within TTL, miss after TTL expiry, cold misses, install replaces snapshot

#### Story 006 — Daemon in-memory suppress (`Arc<AtomicU64>`) ✅

- ✅ `SuppressState { last_ms: AtomicU64 }` with `warm()` (Release store) and `is_suppressed()` (Acquire load → file fallback → promote on file hit)
- ✅ Daemon's own warm calls update `Arc<AtomicU64>` directly without writing the file
- ✅ Cross-process callers (CLI) keep using `suppress_avoider` (file write); daemon's fallback file-stat path picks them up
- ✅ 4 new tests verify cold path returns not-suppressed, warm path skips file IO, external file write is observed via fallback, stale file timestamps are rejected

### Defense-in-depth verification

- **Bolt 026's boundary test** still passes: the new daemon state (ClientCache, SuppressState) was added without introducing any forbidden imports. The `tests/boundary.rs` grep test would have flagged any leak immediately.
- **Daemon single-package build** (`cargo build -p media-control-daemon`, cli off): clean. Confirms no `commands::workflow` or `jellyfin` infrastructure crept in via the new state.

### avoid.rs LOC delta

- **Total file**: 2234 → 1894 (**−340** lines)
- **Production region** (before `mod tests`): ~727 → ~752 (+25 nominal)
- **Production region adjusted**: subtract the new `pub avoid_with_clients` API surface (~15 LOC, didn't exist before) and the `#[cfg(test)]` `rectangles_overlap` shim → roughly **−5 LOC**

The +25 nominal growth is dominated by the new `avoid_with_clients` public API (Story 005's substrate) and load-bearing doc-comments on the named constants. The duplication elimination is real (8-arg fn collapsed; two `overlaps_focused` closures unified; 5 per-arm logs collapsed to one `Display`-driven log; `restore_focus_suppressed` deduplicated; `PositionResolver` collapses 4 resolves into 1 helper) — it just washes against the new helpers + `Display` impl + `dispatch` method at face-value LOC. The **clarity gain is the real win**: a reader doesn't have to track the same logic in three places anymore.

The 1500 → 1142 LOC test-region collapse is real and substantial — the migration to `ClientBuilder` made every test that constructs synthetic clients ~3x shorter.

### Audit hit-list coverage (15 items)

| # | Item | Story | Status |
|---|---|---|---|
| 1 | Plumb `is_minified()` once | 002 | ✅ |
| 2 | Duplicate `overlaps_focused` closures | 001 | ✅ |
| 3 | `get_position_pair` + `calculate_target_position` rebuild closures | 002 | ✅ |
| 4 | `get_clients()` round-trips per event | 005 | ✅ |
| 5 | `media_windows: Vec` allocates each tick | 005 | ✅ (folded into cache) |
| 6 | `avoid()` god-function | 003 | ✅ (collapsed to delegate-to-`avoid_with_clients`) |
| 7 | `classify_case` + `match case` redundant double dispatch | 003 | ✅ |
| 8 | `suppress_avoider()` + `restore_focus()` pair duplicated 2× | 003 | ✅ |
| 9 | Magic numbers without constants | 003 | ✅ |
| 10 | `should_suppress` does file IO every tick | 006 | ✅ |
| 11 | `FocusedWindow` is a copy of `Client` with bools | 003 | ⚠️ **deferred** — cost-benefit didn't favor |
| 12 | Test scenario builders should move to test_helpers | 004 | ✅ |
| 13 | `assert_handler_warms_suppression` env-mutex dance | 004 | ✅ |
| 14 | `tracing::debug!` repeated per arm | 003 | ✅ |
| 15 | `rectangles_overlap` 8-arg signature | 001 | ✅ |

**14/15 landed.** Item 11 deferred with rationale documented (FocusedWindow already only carries cheap data; converting to borrowed view shifts work to accessors and adds lifetime noise across handler signatures for ~20 LOC win).

### "What NOT to touch" backstop — verified preserved

- ✅ `i64` widening in `Rect::overlaps` (which subsumed `rectangles_overlap`), `within_tolerance`, `calculate_target_position` — all preserved; overflow tests pass
- ✅ Three diagnostic branches in `should_suppress` — untouched
- ✅ Double-suppress in `handle_mouseover_geometry` and `handle_mouseover_toggle` — untouched (the deduplication via `restore_focus_suppressed` covered the *non-mouseover* sites; mouseover handlers still use the explicit double-suppress pattern intentionally)
- ✅ Scratchpad `monitor < 0` early-return — preserved with new named constant `SCRATCHPAD_MONITOR`; regression test still passes
- ✅ `fullscreen > 0` not "simplified" to `== 1` — preserved with new named constant `FULLSCREEN_NONE` (the check is now `fullscreen != FULLSCREEN_NONE`, which is semantically identical but reads more intentionally)

### Issues Found

None introduced by this bolt.

The pre-existing test flake from bolt 025 (`commands::window::move_window::tests::move_down_dispatches_correct_position` racing under parallel runs) **may now be implicitly fixed** — story 004's `with_isolated_runtime_dir` primitive is exactly the right tool for it. Not explicitly verified in this bolt; flagged for follow-up.

### Notes

- **Test count growth (+13)** comes from real new behavior coverage (5 geometry tests + 4 ClientCache + 4 SuppressState), not test-bloat.
- **Doc-tests unchanged at 16** — none of the `///` doc-comments referencing internal APIs broke during the refactor.
- **Daemon's hot-path discipline**: the implementation agent observed and respected the suppress-first / cache-second / `avoid_with_clients` / `suppress.warm()` ordering, which minimizes work on the suppressed-tick fast path. Worth preserving in any future event-loop changes.
- **`debounce_ms` reuse**: cache TTL reads from `config.positioning.debounce_ms` rather than introducing a new constant. One source of truth, no drift hazard.
- **Boundary integrity**: bolt 026's `tests/boundary.rs` runs as part of `cargo test --workspace`. Any future contributor adding a forbidden import gets an immediate signal; the daemon's new state didn't perturb this.
