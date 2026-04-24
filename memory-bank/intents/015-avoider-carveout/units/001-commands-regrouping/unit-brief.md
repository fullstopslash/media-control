---
unit: 001-commands-regrouping
intent: 015-avoider-carveout
phase: inception
status: ready
created: 2026-04-26T00:00:00Z
updated: 2026-04-26T00:00:00Z
---

# Unit Brief: commands-regrouping

## Purpose

Carve the flat `commands/` namespace into two visibly separate subnamespaces — `commands::window` (avoider-relevant) and `commands::workflow` (CLI-only) — plus a `commands::shared` helper module. This is the foundational refactor; nothing else in the intent is possible until the boundary exists in code.

## Scope

### In Scope

- Create `commands/window/` and `commands/workflow/` directories
- Move 8 command files into `window/`: avoid, fullscreen, move_window, pin, minify, focus, close, keep
- Move 6 command files into `workflow/`: mark_watched, chapter, play, seek, status, random
- Extract or relocate the shared helpers currently in `commands/mod.rs` (`get_suppress_file_path`, `move_pixel_action`, `now_unix_millis`, `resize_pixel_action`, `restore_focus`, `suppress_avoider`, `send_mpv_script_message`, `send_mpv_ipc_command`, `runtime_dir`, `CommandContext`)
- Update `commands/mod.rs` re-exports so existing `use media_control_lib::commands::{avoid, ...}` paths in CLI and daemon keep compiling unchanged via `pub use` shims (or update call sites — design-stage decision)

### Out of Scope

- Any signature, logic, or behavior change inside the moved files
- Renaming functions or types
- Adding visibility restrictions (that's unit 002)
- Touching `jellyfin.rs` (it stays at lib root; only its access pattern changes when callers move into `workflow/`)

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Group commands into `window`, `workflow`, `shared` subnamespaces | Must |

---

## Domain Concepts

### Key Entities

| Entity | Description | Attributes |
|--------|-------------|------------|
| `commands::window` | Subnamespace for window-management commands the daemon legitimately needs | 8 modules |
| `commands::workflow` | Subnamespace for CLI-only mpv/Jellyfin commands | 6 modules |
| `commands::shared` | Subnamespace (or single file) for helpers used by both groups (or just one of them, but already extracted) | ~10 functions/types |

### Key Operations

| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| Move file | `git mv crates/.../commands/X.rs crates/.../commands/{window\|workflow}/X.rs` and update `mod.rs` declarations | filename, target subdir | updated module tree |
| Re-export shim | Optional `pub use window::avoid;` in `commands/mod.rs` to preserve existing import paths | none | back-compat for any external caller (only `media-control` and `media-control-daemon` exist) |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 3 |
| Must Have | 3 |
| Should Have | 0 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-define-submodule-layout | Decide on submodule structure and write empty scaffolding | Must | Planned |
| 002-move-window-commands | Relocate the 8 window-management command files | Must | Planned |
| 003-move-workflow-commands | Relocate the 6 workflow command files and migrate shared helpers | Must | Planned |

---

## Dependencies

### Depends On

| Unit | Reason |
|------|--------|
| (none) | First unit |

### Depended By

| Unit | Reason |
|------|--------|
| 002-daemon-substrate-tightening | Needs the `commands::window` boundary to exist before it can enforce "daemon imports only window" |
| 003-avoider-cleanup | Some cleanup items (test_helpers migration, suppress-state ownership) read more naturally once the modules are grouped |

### External Dependencies

None.

---

## Technical Context

### Suggested Technology

Rust module system. No new crates. Either:

- **Option A: Submodule directories.** `commands/window/{mod.rs, avoid.rs, ...}`. Cleanest hierarchy; matches Rust 2024 edition file conventions.
- **Option B: Single-file submodules.** `commands/window.rs` with `mod avoid; mod fullscreen; ...`. Flatter on disk but `commands/window.rs` becomes a re-export hub.

Option A is preferred (clearer ownership; `git mv` preserves history; matches existing structure where each command is its own file).

### Integration Points

| Integration | Type | Protocol |
|-------------|------|----------|
| `crates/media-control/src/main.rs` | Module import | `use media_control_lib::commands::...` |
| `crates/media-control-daemon/src/main.rs` | Module import | `use media_control_lib::commands::...` |

---

## Constraints

- **Zero behavior change.** This unit is a textbook mechanical refactor; any logic edit is a scope violation.
- **Preserve git history per file.** Use `git mv` (or jj equivalent — `jj` tracks renames automatically) so blame survives.
- **Re-export back-compat shims are acceptable but must not become permanent.** If shims are added, the bolt logs that they should be removed in a follow-up; the goal is for call sites to use the new paths directly.

---

## Success Criteria

### Functional

- [ ] `crates/media-control-lib/src/commands/window/` contains the 8 window-management command files
- [ ] `crates/media-control-lib/src/commands/workflow/` contains the 6 workflow command files
- [ ] Shared helpers live in one obvious place (`commands/shared.rs` or `commands/shared/mod.rs`)
- [ ] `cargo build --workspace` succeeds with zero warnings introduced by this unit
- [ ] `cargo test --workspace` passes with no test modifications (or only mechanical `use`-path updates)

### Non-Functional

- [ ] No file's logic changed (`jj diff` shows only moves + import updates)
- [ ] Call site updates in `media-control/src/main.rs` and `media-control-daemon/src/main.rs` are mechanical path updates, not refactors

### Quality

- [ ] All acceptance criteria met
- [ ] Diff is reviewable in one sitting (mostly `git mv`-style renames)

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 025-commands-regrouping | simple-construction-bolt | 001, 002, 003 | Mechanical module reorganization; one bolt; one PR |

---

## Notes

The coupling survey (2026-04-26) confirms zero cross-cuts between window-mgmt and workflow command files. The split is a clean cut, not a careful disentanglement. Estimate: small bolt, mostly `git mv` and `mod` declarations.
