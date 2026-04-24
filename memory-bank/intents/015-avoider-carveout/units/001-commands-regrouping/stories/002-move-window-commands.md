---
id: 002-move-window-commands
unit: 001-commands-regrouping
intent: 015-avoider-carveout
status: ready
priority: must
created: 2026-04-26T00:00:00Z
assigned_bolt: 025-commands-regrouping
implemented: false
---

# Story: 002-move-window-commands

## User Story

**As a** developer carving the avoider away from workflow code
**I want** the 8 window-management command files relocated into `commands/window/`
**So that** the avoider-relevant subset of `commands` is visibly grouped, and the daemon's import surface is one namespace

## Acceptance Criteria

- [ ] **Given** the empty `commands/window/` scaffolding from story 001, **When** I move `avoid.rs`, `fullscreen.rs`, `move_window.rs`, `pin.rs`, `minify.rs`, `focus.rs`, `close.rs`, `keep.rs` into `commands/window/`, **Then** `cargo build --workspace` and `cargo test --workspace` pass
- [ ] **Given** the moves, **When** call sites in `crates/media-control/src/main.rs` and `crates/media-control-daemon/src/main.rs` are updated to use the new paths (or back-compat `pub use` shims are added in `commands/mod.rs`), **Then** no unrelated logic edits land in those files
- [ ] **Given** the moves, **When** `jj diff` is reviewed, **Then** changes are limited to: file renames, `mod` declaration updates, and `use`-path updates

## Technical Notes

- Use `jj` for the renames; do not edit file contents during the move
- Recommended order: move one file (e.g., `keep.rs`, the smallest), confirm `cargo build` passes, then bulk-move the rest. This isolates any unexpected coupling.
- `focus.rs` imports `suppress_avoider` from `commands/mod.rs` — keep that import path working during this story (story 003 handles the shared helpers extraction)

## Dependencies

### Requires

- 001-define-submodule-layout

### Enables

- 003-move-workflow-commands (can run in parallel in practice, but easier to stage)
- All stories in unit 003 (avoider-cleanup)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `focus.rs` references workflow-side intent ("focus media window or launch Jellyfin") in its doc comment | Doc comment stays; no logic crosses into workflow code |
| Tests in moved files reference helpers from sibling modules | Update `use super::...` paths as needed; no test logic changes |

## Out of Scope

- Moving workflow commands (story 003)
- Extracting `commands/mod.rs` helpers into `commands/shared.rs` (story 003)
- Any cleanup or refactor inside the moved files (unit 003)
