---
id: 022-audit-mark-watched-tests
unit: 001-audit-fixes
intent: 014-audit-round4-fixes
type: simple-construction-bolt
status: complete
stories:
  - mark-watched-test-coverage
created: 2026-04-23T00:00:00Z
completed: 2026-04-23T22:32:29Z
status_backfilled: 2026-04-29T12:00:00Z
source_commit: a9a0ce86
requires_bolts: []
enables_bolts: []
requires_units: []
blocks: false

complexity:
  avg_complexity: 1
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

## Bolt: 022-audit-mark-watched-tests

### Objective
Bring `commands/mark_watched.rs` from zero test coverage to full coverage.
Single-file scope.

### Stories Included

- [ ] **mark-watched-test-coverage** — Add `#[cfg(test)] mod tests` covering:
  1. `mark_watched` with no mpv socket → `MpvIpc(NoSocket)` returned
  2. `mark_watched_and_stop` partial-failure path: mark succeeds, stop fails →
     `warn!` is emitted (assert via `tracing-test` if available, else just
     verify error returned) and the stop error propagates
  3. `mark_watched_and_next` calls underlying mpv IPC once (single-step path)
  4. `mark_watched_and_stop` happy path with both calls succeeding

  Use the established mock-mpv-socket pattern (look for the existing fixture
  in commands/mod.rs or close.rs tests).

### Expected Outputs
- mark_watched.rs only (or shared test fixture imported)
- 4 tests
- `cargo test --workspace` clean

### Dependencies
None. Pure test addition, no behavior changes.

### Completion (status backfilled 2026-04-29)

Frontmatter sync — work shipped in commit `a9a0ce86` (2026-04-23). Verified
2026-04-29: `mod tests` at `commands/workflow/mark_watched.rs:111` with 4
`#[tokio::test]` cases covering the no-socket, partial-failure, and happy
paths.
