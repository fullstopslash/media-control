---
intent: 015-avoider-carveout
phase: inception
status: units-decomposed
updated: 2026-04-26T00:00:00Z
---

# Avoider Daemon Carve-Out - Unit Decomposition

## Units Overview

This intent decomposes into **3 units**, executed sequentially. Each unit produces a green `cargo test --workspace` and zero behavior change at its end (cleanup excepted, where the change is intentional and bounded).

### Unit 1: 001-commands-regrouping

**Description**: Split the flat `commands/` namespace into `commands::window`, `commands::workflow`, and `commands::shared`. Pure module-move refactor — no signature changes, no logic changes, no new types. Update `commands/mod.rs` re-exports so existing call sites in CLI and daemon keep compiling unchanged.

**Stories**:

- 001-001-define-submodule-layout: Decide the exact module tree (window/, workflow/, shared/ vs. shared.rs) and write the empty scaffolding.
- 001-002-move-window-commands: Relocate avoid, fullscreen, move_window, pin, minify, focus, close, keep into `commands/window/`.
- 001-003-move-workflow-commands: Relocate mark_watched, chapter, play, seek, status, random into `commands/workflow/`. Move `commands/mod.rs` shared helpers (`get_suppress_file_path`, `move_pixel_action`, `now_unix_millis`, `resize_pixel_action`, `restore_focus`, `suppress_avoider`, `send_mpv_script_message`, `send_mpv_ipc_command`, `runtime_dir`) into `commands/shared.rs` (or keep in mod.rs as `pub use shared::*`).

**Deliverables**:

- New module tree under `crates/media-control-lib/src/commands/`
- `cargo build --workspace` and `cargo test --workspace` pass with zero modifications elsewhere

**Dependencies**:

- Depends on: None
- Depended by: 002-daemon-substrate-tightening, 003-avoider-cleanup

**Estimated Complexity**: M

---

### Unit 2: 002-daemon-substrate-tightening

**Description**: Make the daemon's "no workflow, no Jellyfin" property a build-enforced contract. Audit `crates/media-control-daemon/Cargo.toml` and the daemon's `use` statements. Choose and apply one enforcement mechanism (the bolt's design stage picks: cargo feature flag gating workflow modules behind a `cli` feature; or `pub(crate)` boundaries that make `commands::workflow` invisible from outside the lib's CLI-facing API; or a `compile_fail` doctest at the daemon root that proves importing workflow breaks). Verify with `cargo tree -p media-control-daemon`.

**Stories**:

- 002-001-pick-and-apply-enforcement: Survey the three options against existing project conventions, pick one, apply it. Document the choice in the bolt's decision log.
- 002-002-prove-isolation: Write the verification test (form depends on choice above): a `compile_fail` doctest, a CI grep, or a `cargo tree` assertion. Confirm `reqwest` is not pulled into the daemon's resolved dependency set if the chosen mechanism allows feature-gating it.

**Deliverables**:

- One enforcement mechanism applied and documented
- Verification test that fails CI if a workflow import is added to the daemon
- Updated `crates/media-control-daemon/Cargo.toml` if features are used

**Dependencies**:

- Depends on: 001-commands-regrouping (needs the `commands::window` / `commands::workflow` boundary to exist)
- Depended by: 003-avoider-cleanup (cleanup may rely on the daemon owning its own state, which assumes the boundary is firm)

**Estimated Complexity**: S

---

### Unit 3: 003-avoider-cleanup

**Description**: Apply the prioritized hit list from the avoid.rs audit. Two scopes: (a) intra-`avoid.rs` DRY/clarity items that don't need daemon state (Rect newtype, plumbed-once `is_minified`, deduplicated `restore_focus_suppressed`, `classify_case` collapse, named constants, scenario-builder migration to `test_helpers.rs`); (b) daemon-owned hot-path state (cached `get_clients()`, in-memory suppress state, reused `Vec<MediaWindow>` buffer) — this is what the carve-out enables.

**Stories**:

- 003-001-rect-newtype-and-overlap-helpers: Introduce `Rect { x, y, w, h }` with `overlaps(&Rect)`. Replace 8-arg `rectangles_overlap` and the two duplicate `overlaps_focused` closures.
- 003-002-plumb-minified-and-position-resolver: Compute `is_minified()` once per `avoid()` call; pass through to `move_media_window`, `try_move_clear_of`, `handle_move_to_primary` loop, `handle_fullscreen_nonmedia`, `handle_geometry_overlap`. Introduce `PositionResolver { ctx, minified }` so `get_position_pair` and `calculate_target_position` stop rebuilding the closure.
- 003-003-collapse-classify-dispatch-and-restore-focus-helper: Inline `classify_case` into `avoid()` (or have `AvoidCase` carry `dispatch(self, ...)`); extract `restore_focus_suppressed(ctx, addr)`; replace per-arm `tracing::debug!` lines with one `Display`-driven log; introduce named constants for `0`/`100`/`-1`.
- 003-004-migrate-scenario-builders: Move repeated `make_test_client_full(...)`-blocks and `assert_handler_warms_suppression` env-mutex dance from `avoid.rs` tests into `test_helpers.rs` as a `ClientBuilder` and `with_isolated_runtime_dir` primitive.
- 003-005-daemon-cached-clients: In the daemon, cache the `get_clients()` result; invalidate on `openwindow`/`closewindow`/`movewindow` events (or fall back to TTL-of-one-debounce if event-based proves unreliable — design-stage decision).
- 003-006-daemon-in-memory-suppress: In the daemon, hold suppress state in `Arc<AtomicU64>`; the file-based `should_suppress` remains for cross-process callers but the daemon's own writes update the in-memory copy directly.

**Deliverables**:

- `avoid.rs` strictly smaller (production code)
- Daemon main loop holds explicit cache state with documented invalidation
- All existing tests pass; new tests cover daemon-owned state correctness
- `test_helpers.rs` grows new builders; no parallel mock module exists in the daemon

**Dependencies**:

- Depends on: 002-daemon-substrate-tightening (daemon can confidently hold state without leaking it back into the lib's stateless contract)
- Depended by: None

**Estimated Complexity**: L

---

## Requirement-to-Unit Mapping

- **FR-1** (Group commands by concern) → 001-commands-regrouping
- **FR-2** (Daemon depends only on substrate + window) → 002-daemon-substrate-tightening
- **FR-3** (Cleanup pass on avoider hot path) → 003-avoider-cleanup
- **FR-4** (Daemon-owned hot-path state) → 003-avoider-cleanup
- **FR-5** (Test infrastructure stays single-source) → 003-avoider-cleanup (story 003-004 specifically; constraint observed throughout)

## Unit Dependency Graph

```text
[001-commands-regrouping] ──> [002-daemon-substrate-tightening] ──> [003-avoider-cleanup]
```

Strict linear chain. Each unit's preconditions are the prior unit's deliverables.

## Execution Order

1. **001-commands-regrouping** — pure mechanical move; quickest to land; unblocks everything.
2. **002-daemon-substrate-tightening** — small surface; one design decision; one enforcement mechanism.
3. **003-avoider-cleanup** — the meaty work; six stories; the user's stated priority.
