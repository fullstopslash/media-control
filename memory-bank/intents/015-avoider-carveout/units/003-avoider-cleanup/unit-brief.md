---
unit: 003-avoider-cleanup
intent: 015-avoider-carveout
phase: inception
status: ready
created: 2026-04-26T00:00:00Z
updated: 2026-04-26T00:00:00Z
---

# Unit Brief: avoider-cleanup

## Purpose

Apply the prioritized cleanup hit list to `avoid.rs` and the daemon event loop. Two scopes: (a) intra-`avoid.rs` DRY/clarity items that don't need daemon state; (b) daemon-owned hot-path state that's only safe to add now that the daemon's surface is contractually sealed (per unit 002).

This is the user's stated priority for the intent: "really pay special attention to the avoider daemon being absolutely efficient, clean, and DRY."

## Scope

### In Scope

**Intra-`avoid.rs` cleanup:**

- Introduce `Rect { x, y, w, h }` with `overlaps(&Rect)`; replace the 8-arg `rectangles_overlap` and the two duplicate `overlaps_focused` closures (audit items 2, 15)
- Compute `is_minified()` once per `avoid()` call and plumb through; eliminate the per-window/per-handler restat (audit item 1)
- Introduce `PositionResolver { ctx, minified }` so `get_position_pair` and `calculate_target_position` stop rebuilding their resolve closure (audit item 3)
- Inline `classify_case` into `avoid()` (or have `AvoidCase` carry `dispatch(self, ...)`) — eliminate the build-then-immediately-match double-dispatch (audit item 7)
- Extract `restore_focus_suppressed(ctx, addr)`; collapse the duplicated suppress-then-restore pair at handler 615-618 / 651-659 (audit item 8)
- Replace per-arm `tracing::debug!("avoid: case=...")` lines with a single `Display`-driven log (audit item 14)
- Introduce named constants: `FULLSCREEN_NONE: u8 = 0`, `PERCENT_MAX: u16 = 100`, `SCRATCHPAD_MONITOR: i32 = -1` (audit item 9)
- Replace `FocusedWindow` (today a copy of `Client` with bools precomputed) with `struct FocusedWindow<'a> { client: &'a Client, is_media: bool }` or pass as `(&Client, bool)` (audit item 11)
- Migrate scenario builders (`scenario_single_workspace`, repeated `make_test_client_full(...)` blocks) and `assert_handler_warms_suppression` env-mutex dance from `avoid.rs` tests into `test_helpers.rs` as a `ClientBuilder` and `with_isolated_runtime_dir` primitive (audit items 12, 13)

**Daemon-owned hot-path state:**

- Cache `get_clients()` results across debounced events; invalidate on `openwindow`/`closewindow`/`movewindow` events (audit item 4; design-stage decision: event-driven invalidation vs. TTL-of-one-debounce)
- Hold suppress state in `Arc<AtomicU64>` inside the daemon; the file-based `should_suppress` remains for cross-process callers but the daemon's own suppress writes update the in-memory copy directly (audit item 10)
- Reuse a `Vec<MediaWindow>` buffer across iterations of the avoidance loop, or convert `find_media_windows` to return `impl Iterator` (audit item 5)
- Optionally: extract `prepare_avoid_input(ctx) -> Option<AvoidInput>` from the god-function `avoid()` so the 5 case handlers become individually testable (audit item 6) — defer if scope creep, but it's the natural pair to caching `get_clients()`

### Out of Scope

- The "what NOT to touch" list from the audit:
  - The `i64` widening in `rectangles_overlap`, `within_tolerance`, `calculate_target_position` (overflow-defense; has dedicated tests)
  - The three diagnostic branches in `should_suppress` (operator log distinguishability)
  - The double-suppress in `handle_mouseover_geometry` and `handle_mouseover_toggle` (intentional race fix)
  - The scratchpad `monitor < 0` early-return (regression fix with its own test)
  - Any "simplification" of `fullscreen > 0` to `== 1` (Hyprland uses 0/1/2/3)
- Behavior changes to avoidance logic
- New CLI subcommands or new daemon behaviors
- Touching `jellyfin.rs` or workflow commands

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-3 | Apply prioritized cleanup pass to avoider hot path | Must |
| FR-4 | Daemon-owned hot-path state (cache, in-memory suppress, buffer reuse) | Should |
| FR-5 | Test infrastructure stays single-source (test_helpers.rs is the one place) | Must |

---

## Domain Concepts

### Key Entities

| Entity | Description | Attributes |
|--------|-------------|------------|
| `Rect` | Newtype for rectangle geometry | `x, y, w, h: i32`; `overlaps(&Rect) -> bool` |
| `PositionResolver` | Bundles `ctx` + precomputed `minified` for position-resolution closures | Methods: `resolve_or`, `resolve_opt` |
| `FocusedWindow<'a>` | Borrowed view of the focused client + media flag | `client: &'a Client`, `is_media: bool` |
| `ClientCache` (daemon) | Cached `Vec<Client>` with invalidation rules | Invalidated on `openwindow`/`closewindow`/`movewindow` events |
| `SuppressState` (daemon) | In-memory suppress timestamp | `Arc<AtomicU64>` |

### Key Operations

| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| Cached `get_clients` | Return cached `Vec<Client>` if cache valid; refresh on miss | event sequence or invalidation flag | `Vec<Client>` |
| In-memory `should_suppress` | Read `Arc<AtomicU64>`; fall back to file if env var indicates external writer | none | `bool` |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 6 |
| Must Have | 4 |
| Should Have | 2 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-rect-newtype-and-overlap-helpers | Introduce `Rect` newtype; replace 8-arg `rectangles_overlap` and duplicate `overlaps_focused` closures | Must | Planned |
| 002-plumb-minified-and-position-resolver | Compute `is_minified()` once per `avoid()`; introduce `PositionResolver` | Must | Planned |
| 003-collapse-classify-dispatch-and-restore-focus-helper | Inline `classify_case`; extract `restore_focus_suppressed`; replace per-arm debug logs; named constants | Must | Planned |
| 004-migrate-scenario-builders | Move test scenario builders + env-mutex dance from `avoid.rs` to `test_helpers.rs` | Must | Planned |
| 005-daemon-cached-clients | Cache `get_clients()` in daemon with documented invalidation | Should | Planned |
| 006-daemon-in-memory-suppress | Hold suppress state in `Arc<AtomicU64>` in the daemon | Should | Planned |

---

## Dependencies

### Depends On

| Unit | Reason |
|------|--------|
| 001-commands-regrouping | `avoid.rs` lives at its new path under `commands/window/`; test_helpers migration target is unambiguous |
| 002-daemon-substrate-tightening | Daemon-owned state (FR-4) is safer once the daemon's surface is contractually sealed |

### Depended By

| Unit | Reason |
|------|--------|
| (none) | Final unit |

### External Dependencies

None.

---

## Technical Context

### Suggested Technology

Rust. Stories 005 and 006 may want `tokio::sync::watch` or `arc-swap` for cache snapshots; design-stage decision. No mandatory new crates — `Arc<AtomicU64>` and `Mutex<Vec<Client>>` cover the 80% case.

### Integration Points

| Integration | Type | Protocol |
|-------------|------|----------|
| `crates/media-control-lib/src/commands/window/avoid.rs` | In-place edits | Rust |
| `crates/media-control-lib/src/test_helpers.rs` | New helpers added | Rust |
| `crates/media-control-daemon/src/main.rs` | Cache + suppress state added | Rust |

---

## Constraints

- **Behavioral parity is non-negotiable.** Existing avoidance test scenarios pass without modification (modulo helper extraction in story 004 — those tests get *cleaner*, not *different*).
- **No changes to `should_suppress`'s file format or `suppress_avoider`'s file-write semantics.** Cross-process callers (CLI subcommands warming the daemon) depend on the file as the IPC medium. The daemon's in-memory state is a fast path that mirrors the file's intent, not a replacement.
- **No new mock layers.** All test infrastructure additions go in `test_helpers.rs`.
- **Respect the "what NOT to touch" list** from the audit (see Out of Scope).

---

## Success Criteria

### Functional

- [ ] All 7 FR-3 sub-items landed (Rect, plumbed-minified, PositionResolver, collapsed dispatch, restore_focus_suppressed, named constants, scenario-builder migration)
- [ ] FR-4 daemon state landed (cache, in-memory suppress, buffer reuse) with documented invalidation rules
- [ ] `cargo test --workspace` passes
- [ ] Manual smoke test: avoidance behavior on real Hyprland is indistinguishable from pre-cleanup

### Non-Functional

- [ ] `avoid.rs` production code is strictly smaller in LOC (today ~727 LOC; target: meaningful reduction)
- [ ] Per-event syscall count is strictly lower (no new syscalls; suppress-file stat removed from daemon hot path)
- [ ] Per-event allocation count is strictly lower (`Vec<MediaWindow>` reused; `get_clients()` cached)

### Quality

- [ ] All acceptance criteria met
- [ ] Code reviewed (this is the bolt where careful review pays off most)

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 027-avoider-cleanup | DDD (model + design + implement + test stages) | 001-006 | Apply hit list; add daemon-owned state; keep tests green |

A single bolt is appropriate even though there are 6 stories: they're tightly coupled (changing test_helpers.rs while moving scenario builders, plumbing `is_minified` while introducing `PositionResolver`, etc.). Splitting into multiple bolts creates merge-conflict risk for negligible isolation benefit.

If the bolt grows unwieldy during construction, the natural seam is: bolt 027a (stories 001-004, intra-`avoid.rs`) and bolt 027b (stories 005-006, daemon-state). Construction Agent can replan if needed.

---

## Notes

The audit produced 15 hit-list items. Items 2, 15 collapse into story 001. Item 1 → story 002. Items 3 → story 002. Items 7, 8, 9, 14 → story 003. Items 12, 13 → story 004. Item 4 → story 005. Item 10 → story 006. Item 5 → distributed across stories 002/005. Item 11 → folded into story 003 if scope allows; defer otherwise. Item 6 → optional, do if natural during story 005's `prepare_avoid_input` extraction.

The full hit list with file:line citations is in this conversation's record; if construction needs it surfaced, copy it to a `cleanup-hit-list.md` artifact in this unit's directory.
