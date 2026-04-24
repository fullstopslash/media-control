---
id: 001-define-submodule-layout
unit: 001-commands-regrouping
intent: 015-avoider-carveout
status: ready
priority: must
created: 2026-04-26T00:00:00Z
assigned_bolt: 025-commands-regrouping
implemented: false
---

# Story: 001-define-submodule-layout

## User Story

**As a** developer reorganizing the commands namespace
**I want** the empty submodule scaffolding (`window/`, `workflow/`, `shared.rs`) committed first, with `mod.rs` declarations that compile against an empty namespace
**So that** subsequent stories move files into a target that already exists and the diff for each move is small and reviewable

## Acceptance Criteria

- [ ] **Given** the current flat `commands/` layout, **When** I add `commands/window/mod.rs`, `commands/window/Cargo`-style file conventions, `commands/workflow/mod.rs`, and `commands/shared.rs` (or `shared/mod.rs`) as empty/near-empty scaffolding, **Then** `cargo build --workspace` still passes (because nothing references the new modules yet)
- [ ] **Given** the bolt design stage, **When** the layout choice (Option A: directory submodules vs. Option B: single-file submodules) is made, **Then** the decision is recorded in the bolt's construction log with one-paragraph rationale
- [ ] **Given** the chosen layout, **When** I declare the new submodules in `commands/mod.rs`, **Then** `cargo clippy --workspace -- -D warnings` reports no `unused_module` complaints (use `#[allow]` or empty `pub use` shims as needed for the transition)

## Technical Notes

- Recommend Option A (directory submodules `commands/window/{mod.rs, ...}`) per unit brief
- Use `jj` for the moves — it tracks renames automatically and preserves blame
- The empty-scaffolding state is committed *before* file moves so that each subsequent story's diff is purely a move + a `mod` declaration

## Dependencies

### Requires

- None (first story of the unit)

### Enables

- 002-move-window-commands
- 003-move-workflow-commands

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `commands/shared.rs` doesn't exist yet but `commands/mod.rs` declares helpers | Helpers stay in `commands/mod.rs` until story 003; no premature extraction |
| Empty `commands/window/mod.rs` with no submodules declared | Build passes; warning suppressed via `#[allow(dead_code)]` if needed during transition |

## Out of Scope

- Moving any files (that's stories 002 and 003)
- Choosing the FR-2 enforcement mechanism (that's unit 002)
