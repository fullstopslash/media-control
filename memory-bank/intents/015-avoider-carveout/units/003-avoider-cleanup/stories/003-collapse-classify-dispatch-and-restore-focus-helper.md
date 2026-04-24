---
id: 003-collapse-classify-dispatch-and-restore-focus-helper
unit: 003-avoider-cleanup
intent: 015-avoider-carveout
status: ready
priority: must
created: 2026-04-26T00:00:00Z
assigned_bolt: 027-avoider-cleanup
implemented: false
---

# Story: 003-collapse-classify-dispatch-and-restore-focus-helper

## User Story

**As a** maintainer reading `avoid()`
**I want** the redundant double-dispatch (`classify_case` then `match case`) collapsed, the duplicated suppress-then-restore pair extracted into `restore_focus_suppressed()`, the per-arm debug logs replaced with one `Display`-driven log, and magic numbers replaced with named constants
**So that** `avoid()` reads as one straightforward dispatch and the handlers stop carrying ceremony

## Acceptance Criteria

- [ ] **Given** `classify_case` (avoid.rs:388-427) builds an `AvoidCase` enum that `avoid()` (avoid.rs:485-506) immediately matches and dispatches, **When** I refactor, **Then** either (a) classification is inlined into `avoid()` or (b) `AvoidCase` carries a `dispatch(self, ctx, ...)` method — eliminating the build-then-match round-trip
- [ ] **Given** the duplicated suppress-then-restore pair at avoid.rs:615-618 and avoid.rs:651-659, **When** I extract `async fn restore_focus_suppressed(ctx: &CommandContext, addr: &str)`, **Then** both call sites collapse to one line and the helper carries the warn-on-error logic
- [ ] **Given** five identical `tracing::debug!("avoid: case=...")` lines (avoid.rs:487, 491, 495, 499, 503), **When** I replace with a single log driven by `Display` for `AvoidCase`, **Then** there's one log statement, not five
- [ ] **Given** scattered raw `0`, `100`, and `-1` checks (`fullscreen > 0` at avoid.rs:397/407/467, `wide_window_threshold.min(100)` at avoid.rs:171, `monitor < 0` at avoid.rs:448), **When** I introduce constants `FULLSCREEN_NONE: u8 = 0`, `PERCENT_MAX: u16 = 100`, `SCRATCHPAD_MONITOR: i32 = -1`, **Then** the raw values disappear and each constant has a doc-comment explaining the Hyprland convention or regression-test backstop

## Technical Notes

- `FocusedWindow` (avoid.rs:309-345) is a copy of `Client` with bools precomputed. If scope allows, replace with `struct FocusedWindow<'a> { client: &'a Client, is_media: bool }` and accessor methods. If the diff grows unwieldy, defer to a follow-up — call out in construction log.
- `restore_focus_suppressed` should swallow the warn-on-error so the caller doesn't propagate transient suppress failures
- Don't "simplify" `fullscreen > 0` to `== 1` — Hyprland uses 0/1/2/3 fullscreen states; `> 0` means "any fullscreen state" and is correct (audit "what NOT to touch")

## Dependencies

### Requires

- 002-plumb-minified-and-position-resolver

### Enables

- 004-migrate-scenario-builders

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `AvoidCase` variants need different argument shapes for dispatch | If consolidating into a method causes ugly conversions, prefer inlining classification into `avoid()` |
| `restore_focus_suppressed` is called from contexts that want a different log level | Provide a `tracing::Level` parameter or two flavors (`_quiet`, default warn-on-error) — pick the simpler shape |
| A future case-arm needs different suppress timing | The helper covers the two existing call sites; new sites can either use it or open-code with documented justification |

## Out of Scope

- The cached-clients optimization (story 005)
- The in-memory suppress state (story 006)
- The "don't touch" list: `i64` widening, `should_suppress`'s three diagnostic branches, double-suppress in mouseover handlers, scratchpad early-return
