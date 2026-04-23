---
id: 021-audit-minify-fix
unit: 001-audit-fixes
intent: 014-audit-round4-fixes
type: simple-construction-bolt
status: planned
stories:
  - minify-toggle-toctou-fix
  - minify-test-coverage
created: 2026-04-23T00:00:00Z
requires_bolts: []
enables_bolts: []
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 2
  max_dependencies: 1
  testing_scope: 2
---

## Bolt: 021-audit-minify-fix

### Objective
Fix the minify TOCTOU and add full test coverage. Single-file scope:
`crates/media-control-lib/src/commands/minify.rs`.

### Stories Included

- [ ] **minify-toggle-toctou-fix** — `is_minified()` stats the flag file, then
  `toggle_minified()` stats again. Two concurrent invocations can both observe
  `was_minified=false`, both dispatch minified geometry, then second call sees
  the file from the first and removes it — leaves state desynced. Fix: use
  atomic-create (`O_CREAT | O_EXCL`) in a new `try_set_minified(target: bool)`
  helper, treat `AlreadyExists` / `NotFound` as "another concurrent caller
  already toggled, no-op". Toggle the file AFTER successful dispatch.

- [ ] **minify-test-coverage** — Add `#[cfg(test)] mod tests` covering:
  1. Fullscreen window → minify is no-op
  2. No media window → minify is no-op (returns Ok with no dispatch)
  3. Normal toggle on/off (use the existing test mutex pattern from
     commands/context.rs to isolate XDG_RUNTIME_DIR)
  4. Dispatch failure leaves flag unchanged
  5. Toggling back returns window to non-minified geometry

### Expected Outputs
- minify.rs only (test mutex helper imported from context.rs if needed)
- Atomic toggle helper
- 5+ tests
- `cargo test --workspace` clean

### Dependencies
None. Disjoint from bolt 022 (mark_watched.rs).

### Notes
The audit Round 4 explicitly listed both the TOCTOU bug AND zero test coverage
for this file. Both must land together so tests cover the new atomic-toggle
behavior, not the old buggy behavior.
