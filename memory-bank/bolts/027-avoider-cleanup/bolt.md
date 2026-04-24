---
id: 027-avoider-cleanup
unit: 003-avoider-cleanup
intent: 015-avoider-carveout
type: ddd-construction-bolt
status: complete
stories:
  - 001-rect-newtype-and-overlap-helpers
  - 002-plumb-minified-and-position-resolver
  - 003-collapse-classify-dispatch-and-restore-focus-helper
  - 004-migrate-scenario-builders
  - 005-daemon-cached-clients
  - 006-daemon-in-memory-suppress
created: 2026-04-26T00:00:00Z
completed: 2026-04-26T00:00:00Z
requires_bolts: [025-commands-regrouping, 026-daemon-substrate-tightening]
enables_bolts: []
requires_units: [001-commands-regrouping, 002-daemon-substrate-tightening]
blocks: false
current_stage: complete
stages_completed:
  - name: model
    completed: 2026-04-26T00:00:00Z
    artifact: ddd-01-domain-model.md
  - name: design
    completed: 2026-04-26T00:00:00Z
    artifact: ddd-02-technical-design.md
  - name: adr
    completed: 2026-04-26T00:00:00Z
    artifact: (skipped — no ADR-worthy decisions remained after inception)
  - name: implement
    completed: 2026-04-26T00:00:00Z
    artifact: (source code; rust-expert subagent execution; no walkthrough doc per ddd-construction-bolt template)
  - name: test
    completed: 2026-04-26T00:00:00Z
    artifact: ddd-03-test-report.md

complexity:
  avg_complexity: 3
  avg_uncertainty: 2
  max_dependencies: 2
  testing_scope: 3
---

## Bolt: 027-avoider-cleanup

### Objective

Apply the prioritized cleanup hit list (15 items, audit 2026-04-26) to
`avoid.rs` and the daemon event loop. Two scopes: (a) intra-`avoid.rs`
DRY/clarity items; (b) daemon-owned hot-path state that's only safe now that
the daemon's surface is contractually sealed.

This is the user's stated priority for the intent: **"really pay special
attention to the avoider daemon being absolutely efficient, clean, and DRY."**

### Stories Included

- [ ] **001-rect-newtype-and-overlap-helpers** — Introduce `Rect { x, y, w, h }`
  with `overlaps(&Rect)`. Replace 8-arg `rectangles_overlap` (avoid.rs:38-55)
  and the two duplicate `overlaps_focused` closures (avoid.rs:525-536,
  682-693). Preserve `i64` widening (load-bearing for overflow tests at
  avoid.rs:758, 793, 808).

- [ ] **002-plumb-minified-and-position-resolver** — Compute `is_minified()`
  once per `avoid()` call; plumb through `move_media_window` (avoid.rs:202-224),
  `try_move_clear_of`, `handle_move_to_primary` loop (avoid.rs:514),
  `handle_fullscreen_nonmedia` (avoid.rs:716), `handle_geometry_overlap`
  (avoid.rs:677). Introduce `PositionResolver { ctx, minified }` so
  `get_position_pair` (avoid.rs:82-126) and `calculate_target_position`
  (avoid.rs:134-192) stop rebuilding their resolve closure.

- [ ] **003-collapse-classify-dispatch-and-restore-focus-helper** — Inline
  `classify_case` (avoid.rs:388-427) into `avoid()` or have `AvoidCase` carry
  a `dispatch` method (eliminates double-dispatch at avoid.rs:485-506). Extract
  `restore_focus_suppressed(ctx, addr)` to deduplicate the suppress-then-restore
  pair (avoid.rs:615-618, 651-659). Replace 5 per-arm `tracing::debug!` with
  one `Display`-driven log. Introduce constants `FULLSCREEN_NONE`,
  `PERCENT_MAX`, `SCRATCHPAD_MONITOR`. If scope allows, also collapse
  `FocusedWindow` (avoid.rs:309-345) to a borrowed view.

- [ ] **004-migrate-scenario-builders** — Move `scenario_single_workspace`
  (avoid.rs:851-881), the 6 repeated `make_test_client_full(...)` blocks
  (avoid.rs:888-915, 982-1009, 1035-1062, 1083-1110, 1133-1160, 1221-1260),
  and `assert_handler_warms_suppression` env-mutex dance (avoid.rs:2031-2088)
  into `test_helpers.rs` as `ClientBuilder` and `with_isolated_runtime_dir`.
  Per FR-5, no parallel mock module in the daemon.

- [ ] **005-daemon-cached-clients** — In the daemon, cache the `get_clients()`
  result with **TTL-of-one-debounce** (15ms; reuse the existing debounce
  constant). Shape: `Mutex<Option<(Instant, Vec<Client>)>>`. Refetch when
  `now - captured_at >= DEBOUNCE_WINDOW`. No event-taxonomy assumptions; the
  TTL covers burst-fired events (the common case). Add `tracing::debug!` for
  hit/miss/refetch.

- [ ] **006-daemon-in-memory-suppress** — Daemon holds suppress timestamp in
  `Arc<AtomicU64>`. File-based path stays for cross-process callers (CLI
  warming the daemon). Design-stage decision: do daemon's own `suppress_avoider`
  calls still mirror to the file? Recommend "no, in-memory only" if no other
  consumer reads the file.

### Expected Outputs

- `avoid.rs` production code strictly smaller in LOC (today ~727; meaningful
  reduction expected from items 2, 3, 6, 7, 8, 11, 15 of the audit)
- Per-event syscall count strictly lower (suppress-file stat removed from
  daemon hot path)
- Per-event allocation count strictly lower (cached `Vec<Client>`; reused
  `Vec<MediaWindow>` buffer)
- All existing tests pass (modulo helper-extraction mechanical updates in
  story 004)
- New tests cover daemon-state correctness (cache hit/invalidation; in-memory
  suppress freshness)
- `test_helpers.rs` grew new builders; daemon has no `mod test_helpers`
- Construction log records: design choice for cache invalidation strategy
  (story 005); design choice for in-memory suppress mirror behavior (story 006)

### Dependencies

Requires bolts 025 (regrouping) and 026 (tightening).

### Notes

**Scope hazard**: Six stories in one bolt is the upper end. Natural split if
needed: bolt 027a (stories 001-004, intra-`avoid.rs`) + bolt 027b (stories
005-006, daemon-state). Construction Agent should replan if the bolt grows
unwieldy.

**What NOT to touch** (from audit, non-negotiable):
- `i64` widening in `rectangles_overlap`, `within_tolerance`,
  `calculate_target_position` (overflow defense; tested)
- Three diagnostic branches in `should_suppress` (operator log
  distinguishability)
- Double-suppress in `handle_mouseover_geometry` and `handle_mouseover_toggle`
  (intentional race fix)
- Scratchpad `monitor < 0` early-return (regression fix; tested at avoid.rs:1364)
- `fullscreen > 0` (Hyprland uses 0/1/2/3; do not "simplify" to `== 1`)

The full 15-item hit list with file:line citations is in the inception
conversation; if construction needs it persisted, copy to
`memory-bank/intents/015-avoider-carveout/units/003-avoider-cleanup/cleanup-hit-list.md`.
