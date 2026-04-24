---
id: 002-plumb-minified-and-position-resolver
unit: 003-avoider-cleanup
intent: 015-avoider-carveout
status: ready
priority: must
created: 2026-04-26T00:00:00Z
assigned_bolt: 027-avoider-cleanup
implemented: false
---

# Story: 002-plumb-minified-and-position-resolver

## User Story

**As a** maintainer caring about per-event efficiency
**I want** `is_minified()` computed once per `avoid()` call and plumbed through, plus a `PositionResolver { ctx, minified }` struct that bundles the position-resolution closure inputs
**So that** the avoider stops restat'ing the minified marker per window and stops rebuilding the same closure in `get_position_pair` and `calculate_target_position`

## Acceptance Criteria

- [ ] **Given** today's `move_media_window` (avoid.rs:202-224) calling `effective_dimensions(ctx)` without precomputed flag, **When** I plumb a `minified: bool` parameter through `move_media_window`, `try_move_clear_of`, and the loop in `handle_move_to_primary` (avoid.rs:514), **Then** `is_minified()` is called exactly once per `avoid()` invocation (verified by adding a test counter or by reading the call graph)
- [ ] **Given** `get_position_pair` (avoid.rs:82-126) and `calculate_target_position` (avoid.rs:134-192) both build a `resolve_or` closure with shared `(minified, ctx)`, **When** I introduce `PositionResolver { ctx, minified }` with `resolve_or(...)` and `resolve_opt(...)` methods, **Then** the four-position resolve in `calculate_target_position` becomes a single helper call instead of four open-coded lines
- [ ] **Given** `handle_fullscreen_nonmedia` (avoid.rs:716) and `handle_geometry_overlap` (avoid.rs:677) also call `is_minified()`, **When** I refactor, **Then** they receive `minified` from the caller, not by re-stating

## Technical Notes

- `is_minified()` likely stats a marker file (`$XDG_RUNTIME_DIR/media-control-minified` or similar). Verify before refactoring; if it's reading from `Config` instead, the gain is smaller but still real (no closure rebuild).
- `PositionResolver` is a borrow-only struct — give it a `<'a>` lifetime on `ctx` and let it be cheaply constructed at the top of each handler that needs it
- Add a unit test that mocks `is_minified()` to count calls per `avoid()` invocation (using `test_helpers.rs` infrastructure)

## Dependencies

### Requires

- 001-rect-newtype-and-overlap-helpers (cleaner adjacent code to refactor)

### Enables

- 003-collapse-classify-dispatch-and-restore-focus-helper

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `is_minified()` changes during a single `avoid()` call (e.g., user toggles minify mid-event) | Acceptable: `avoid()` reads it once at the top; next event re-reads. This matches the debounce semantics. |
| A handler is called outside `avoid()` (e.g., from a CLI subcommand) | The caller still passes `minified`; CLI subcommands compute it themselves once per invocation |
| `PositionResolver` borrows `ctx` for its lifetime | Acceptable; resolver is a stack-local short-lived struct |

## Out of Scope

- The cached-clients optimization (story 005) — this story is intra-`avoid.rs`
- Removing the file-based suppress check (story 006)
