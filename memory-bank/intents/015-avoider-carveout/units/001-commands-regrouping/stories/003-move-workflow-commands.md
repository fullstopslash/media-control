---
id: 003-move-workflow-commands
unit: 001-commands-regrouping
intent: 015-avoider-carveout
status: ready
priority: must
created: 2026-04-26T00:00:00Z
assigned_bolt: 025-commands-regrouping
implemented: false
---

# Story: 003-move-workflow-commands

## User Story

**As a** developer carving the avoider away from workflow code
**I want** the 6 workflow command files relocated into `commands/workflow/`, and the shared helpers in `commands/mod.rs` migrated to a clear home (`commands/shared.rs` or `commands/shared/mod.rs`)
**So that** the workflow subset of `commands` is visibly grouped, and the helpers used by both groups have one obvious location

## Acceptance Criteria

- [ ] **Given** the empty `commands/workflow/` scaffolding from story 001, **When** I move `mark_watched.rs`, `chapter.rs`, `play.rs`, `seek.rs`, `status.rs`, `random.rs` into `commands/workflow/`, **Then** `cargo build --workspace` and `cargo test --workspace` pass
- [ ] **Given** the moves, **When** the shared helpers (`get_suppress_file_path`, `move_pixel_action`, `now_unix_millis`, `resize_pixel_action`, `restore_focus`, `suppress_avoider`, `send_mpv_script_message`, `send_mpv_ipc_command`, `runtime_dir`, `CommandContext`) are migrated into `commands/shared.rs`, **Then** all callers in `commands/window/` and `commands/workflow/` import them via `use super::shared::...` (or `use crate::commands::shared::...`)
- [ ] **Given** the migration, **When** `jj diff` is reviewed, **Then** the only logic edits are import path updates; no helper bodies are modified

## Technical Notes

- Helpers may stay in `commands/mod.rs` as a `pub mod shared;` re-export if scope-limited, but extracting to `commands/shared.rs` is cleaner and matches the stated layout
- `CommandContext` is a heavy traveler — it's constructed in `media-control/src/main.rs` and `media-control-daemon/src/main.rs`. Confirm both still build after the move.
- The empty-body-drain helpers and other plumbing currently in `commands/mod.rs` (per the audit) are migration targets for this story
- After this story, `commands/mod.rs` should contain only `pub mod window; pub mod workflow; pub mod shared;` (plus any back-compat re-exports decided in story 001)

## Dependencies

### Requires

- 001-define-submodule-layout
- 002-move-window-commands (recommended sequencing; not strictly required)

### Enables

- All stories in unit 002 (daemon-substrate-tightening)
- All stories in unit 003 (avoider-cleanup)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| A workflow command imports a helper that's also used by window commands | Helper goes in `shared.rs`; both groups import from there |
| A helper is used only by window commands | Move it to `commands/window/mod.rs` (or a dedicated file); don't pollute `shared.rs` |
| A helper is used only by workflow commands | Move it to `commands/workflow/mod.rs` (or a dedicated file) |
| `jellyfin.rs` is referenced by workflow commands | `jellyfin.rs` stays at lib root (`crate::jellyfin`); workflow commands continue importing it as before |

## Out of Scope

- Any logic, signature, or behavior change to the migrated helpers (unit 003 does cleanups; this story is purely structural)
- Adding visibility restrictions like `pub(crate)` (unit 002)
- Removing back-compat shims if any were added in story 001 (defer to follow-up)
