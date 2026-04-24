---
id: 025-commands-regrouping
unit: 001-commands-regrouping
intent: 015-avoider-carveout
type: simple-construction-bolt
status: complete
stories:
  - 001-define-submodule-layout
  - 002-move-window-commands
  - 003-move-workflow-commands
created: 2026-04-26T00:00:00Z
completed: 2026-04-26T00:00:00Z
requires_bolts: []
enables_bolts: [026-daemon-substrate-tightening, 027-avoider-cleanup]
requires_units: []
blocks: false
current_stage: complete
stages_completed:
  - name: plan
    completed: 2026-04-26T00:00:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-04-26T00:00:00Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-04-26T00:00:00Z
    artifact: test-walkthrough.md

complexity:
  avg_complexity: 1
  avg_uncertainty: 1
  max_dependencies: 0
  testing_scope: 1
---

## Bolt: 025-commands-regrouping

### Objective

Carve `crates/media-control-lib/src/commands/` into three visibly separate
subnamespaces — `commands::window`, `commands::workflow`, `commands::shared` —
without changing any logic or behavior. This is the foundational refactor that
makes the rest of intent 015 possible.

### Stories Included

- [ ] **001-define-submodule-layout** — Decide directory submodules vs.
  single-file submodules (recommend Option A: directories). Create empty
  scaffolding (`commands/window/mod.rs`, `commands/workflow/mod.rs`,
  `commands/shared.rs`). Commit before any moves. Record the layout decision in
  this bolt's construction log.

- [ ] **002-move-window-commands** — Relocate the 8 window-management command
  files (`avoid`, `fullscreen`, `move_window`, `pin`, `minify`, `focus`,
  `close`, `keep`) into `commands/window/`. Use `jj` so renames are tracked.
  Update `mod` declarations and any `use super::...` paths. No logic edits.

- [ ] **003-move-workflow-commands** — Relocate the 6 workflow command files
  (`mark_watched`, `chapter`, `play`, `seek`, `status`, `random`) into
  `commands/workflow/`. Migrate shared helpers (`get_suppress_file_path`,
  `move_pixel_action`, `now_unix_millis`, `resize_pixel_action`,
  `restore_focus`, `suppress_avoider`, `send_mpv_script_message`,
  `send_mpv_ipc_command`, `runtime_dir`, `CommandContext`) into
  `commands/shared.rs`. After this story, `commands/mod.rs` contains only
  `pub mod window; pub mod workflow; pub mod shared;` plus any back-compat
  re-exports.

### Expected Outputs

- New module tree under `crates/media-control-lib/src/commands/`
- `cargo build --workspace` clean
- `cargo test --workspace` clean
- `jj diff` shows: file renames + `mod` declaration updates + `use`-path
  updates only — zero logic edits
- Construction log records the layout decision (Option A vs. B) with
  one-paragraph rationale

### Dependencies

None.

### Notes

This bolt is mechanical. The coupling survey (2026-04-26) confirmed zero
cross-cuts between window-mgmt and workflow command files, so the split is a
clean cut, not a careful disentanglement. Estimate: a single sitting.
