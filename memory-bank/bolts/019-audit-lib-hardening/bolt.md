---
id: 019-audit-lib-hardening
unit: 001-audit-fixes
intent: 014-audit-round4-fixes
type: simple-construction-bolt
status: planned
stories:
  - error-write-read-iofailed-kind
  - error-non-exhaustive
  - hyprland-safe-component-exact-match
  - mod-restore-focus-warn-on-cleanup-fail
  - mod-scaled-dims-clamp-to-config-bound
  - mod-with-media-window-resolve-dead-code
created: 2026-04-23T00:00:00Z
requires_bolts: []
enables_bolts: []
requires_units: []
blocks: false

complexity:
  avg_complexity: 1
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 1
---

## Bolt: 019-audit-lib-hardening

### Objective
Tighten the lib-layer plumbing: error type fidelity, path-component validation
correctness, log-level appropriateness for cursor-cleanup failures, dead-code
resolution. All edits live in error.rs, hyprland.rs, commands/mod.rs.

### Stories Included

- [ ] **error-write-read-iofailed-kind** — Add `HyprlandIpcErrorKind::IoFailed` variant.
  Map `HyprlandError::WriteFailed` and `ReadFailed` to it (currently both map to
  `ConnectionFailed`, which is semantically wrong: a connect failed vs. an I/O
  failure on an established stream). Add unit tests for both bridge arms.

- [ ] **error-non-exhaustive** — Add `#[non_exhaustive]` to `MediaControlError`,
  `MpvIpcErrorKind`, `HyprlandIpcErrorKind` so future variants are not silent
  source-breaking changes.

- [ ] **hyprland-safe-component-exact-match** — Replace `s.contains("..")` in
  `is_safe_component` with `s != ".."` (exact match). The path separators are
  already rejected; bare `..` exact-match is the right guard for a single
  component.

- [ ] **mod-restore-focus-warn-on-cleanup-fail** — In `restore_focus`, when the
  fallback batch send fails, upgrade the swallow log from `tracing::debug!` to
  `tracing::warn!` so operators can diagnose stuck `cursor:no_warps true` state.

- [ ] **mod-scaled-dims-clamp-to-config-bound** — In `scaled_dims`, change the
  upper clamp from `10.0` to `1.0` to match the config-validated bound on
  `minified_scale`. Add a `debug_assert!(raw_scale > 0.0 && raw_scale <= 1.0)`.

- [ ] **mod-with-media-window-resolve-dead-code** — `with_media_window` is
  `#[allow(dead_code)]` with no callers across three audits. Decision: DELETE.
  If future migration is desired, re-introduce when the first caller exists.

### Expected Outputs
- error.rs: new variant + non_exhaustive + tests
- hyprland.rs: exact-match guard
- commands/mod.rs: log upgrade, clamp tightening, dead-code removal
- `cargo check --workspace` clean
- `cargo test --workspace` clean

### Dependencies
None. Touches disjoint sections from other bolts (other bolts edit specific
function bodies, not the surrounding error/IPC plumbing).
