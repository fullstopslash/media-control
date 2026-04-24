---
id: 001-rect-newtype-and-overlap-helpers
unit: 003-avoider-cleanup
intent: 015-avoider-carveout
status: ready
priority: must
created: 2026-04-26T00:00:00Z
assigned_bolt: 027-avoider-cleanup
implemented: false
---

# Story: 001-rect-newtype-and-overlap-helpers

## User Story

**As a** maintainer reading `avoid.rs`
**I want** rectangle geometry expressed as a `Rect { x, y, w, h }` newtype with an `overlaps(&Rect)` method
**So that** the 8-argument `rectangles_overlap` and the two duplicate `overlaps_focused` closures collapse into a single, named, testable abstraction

## Acceptance Criteria

- [ ] **Given** today's `rectangles_overlap(x1, y1, w1, h1, x2, y2, w2, h2) -> bool` (avoid.rs:38-55), **When** I introduce `struct Rect { x: i32, y: i32, w: i32, h: i32 }` with `Rect::overlaps(&self, other: &Rect) -> bool`, **Then** all callers use `rect_a.overlaps(&rect_b)` and the 8-arg function is removed (or kept as a private 2-line shim that constructs `Rect`s and delegates)
- [ ] **Given** the duplicate `overlaps_focused` closures at avoid.rs:525-536 and avoid.rs:682-693, **When** I refactor, **Then** both call sites use a single `Rect`-based helper (a free function or a `Rect::overlaps_focused(focused: &Client) -> bool` extension)
- [ ] **Given** the existing overflow-defense tests at avoid.rs:758, 793, 808, **When** I run them after refactoring, **Then** they pass without modification beyond constructor calls (the `i64` widening must survive the refactor — see "What NOT to touch")

## Technical Notes

- `Rect`'s `overlaps` must preserve the `i64` widening from today's `rectangles_overlap`. Implement as:
  ```rust
  impl Rect {
      fn overlaps(&self, other: &Rect) -> bool {
          let (ax1, ay1) = (self.x as i64, self.y as i64);
          let (ax2, ay2) = (ax1 + self.w as i64, ay1 + self.h as i64);
          // ... same logic, just with named bindings
      }
  }
  ```
- Place `Rect` in `crates/media-control-lib/src/window.rs` if it's reusable, or in a new private module if scoped to avoidance
- Don't introduce a `From<&Client> for Rect` unless it actually deduplicates — many call sites pass the four ints directly

## Dependencies

### Requires

- All stories in unit 001 (commands-regrouping) — `avoid.rs` lives at its new path
- All stories in unit 002 (daemon-substrate-tightening) — the daemon's contract is sealed

### Enables

- 002-plumb-minified-and-position-resolver (cleaner code to plumb through)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `Rect` width or height is zero or negative (degenerate) | `overlaps` returns `false`; matches today's behavior — verify against the overflow tests |
| Hyprland sends adversarial geometry (`i32::MIN`) | `i64` widening prevents `abs()` panic; existing tests cover this |
| `Rect` is constructed from an `&Client` field-by-field | Acceptable; do not force a `From` impl unless it simplifies more than 3 call sites |

## Out of Scope

- Removing the `i64` widening (load-bearing per audit "what NOT to touch")
- Adding methods like `Rect::area`, `Rect::contains_point`, etc., unless a current call site needs them
- Making `Rect` `Copy` if it forces unrelated changes (it probably should be `Copy`, but only if natural)
