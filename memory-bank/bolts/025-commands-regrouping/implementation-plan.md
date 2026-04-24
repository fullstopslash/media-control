---
stage: plan
bolt: 025-commands-regrouping
created: 2026-04-26T00:00:00Z
---

## Implementation Plan: commands-regrouping

### Objective

Reorganize `crates/media-control-lib/src/commands/` from one flat namespace into three visibly separate subnamespaces — `window/`, `workflow/`, `shared/` — without changing any logic or behavior. Foundational refactor that unblocks bolts 026 and 027.

### Deliverables

- `crates/media-control-lib/src/commands/window/{mod.rs, avoid.rs, fullscreen.rs, move_window.rs, pin.rs, minify.rs, focus.rs, close.rs}` — 7 window-management commands + window-internal helpers
- `crates/media-control-lib/src/commands/workflow/{mod.rs, mark_watched.rs, chapter.rs, play.rs, seek.rs, status.rs, random.rs, keep.rs}` — 7 workflow commands + workflow-internal mpv-IPC plumbing
- `crates/media-control-lib/src/commands/shared.rs` — items used by both groups (`CommandContext`, `runtime_dir`, `async_env_test_mutex`)
- `crates/media-control-lib/src/commands/mod.rs` reduced to `pub mod window; pub mod workflow; pub mod shared;` plus back-compat re-exports
- `cargo build --workspace` and `cargo test --workspace` green at end

### Dependencies

None. Pure structural refactor inside the lib crate.

### Technical Approach

**Layout choice (Story 001-001):** Option A — directory submodules (`window/mod.rs`, `workflow/mod.rs`). Matches existing per-file convention; `jj` tracks renames automatically; preserves blame.

**Helper classification** (the substantive part of this refactor — the file moves are mechanical, this map is what makes the moves correct):

| Item (in `commands/mod.rs` today) | Lives in | Reason |
|---|---|---|
| `CommandContext` (struct) | `commands/shared.rs` | Constructed by both CLI binaries; used by every command |
| `runtime_dir()` | `commands/shared.rs` | Used by daemon (substrate-tightening unit will rely on this) and by both command groups |
| `async_env_test_mutex()` (`pub(crate)`) | `commands/shared.rs` | Tests in both groups hold this lock; Story 003-004 will migrate to `test_helpers.rs` |
| `get_media_window`, `get_media_window_with_clients` | `commands/window/mod.rs` | Window-mgmt only |
| `get_suppress_file_path`, `suppress_avoider`, `clear_suppression`, `write_suppress_file` | `commands/window/mod.rs` | Window-mgmt only (suppress is an avoider concept) |
| `restore_focus` | `commands/window/mod.rs` | Window-mgmt only |
| `get_minify_state_path`, `is_minified`, `toggle_minified` | `commands/window/mod.rs` | Window-mgmt only |
| `effective_dimensions`, `resolve_effective_position`, `scaled_dims` | `commands/window/mod.rs` | Window-mgmt only |
| `focus_window_action`, `move_pixel_action`, `resize_pixel_action`, `assert_valid_addr` | `commands/window/mod.rs` | Hyprland-dispatch string builders; window-mgmt only |
| `now_unix_millis` | `commands/window/mod.rs` | Used only by `suppress_avoider`; window-mgmt only |
| `send_mpv_script_message`, `send_mpv_script_message_with_args`, `send_mpv_ipc_command`, `query_mpv_property` | `commands/workflow/mod.rs` | mpv IPC only; workflow only |
| `is_unix_socket`, `validate_mpv_socket_path`, `mpv_socket_paths`, `connect_and_write`, `mpv_ipc_exchange`, `try_mpv_paths` | `commands/workflow/mod.rs` | mpv socket plumbing; workflow only |
| `MPV_IPC_SOCKET_DEFAULT` const | `commands/workflow/mod.rs` | Used only by mpv socket plumbing |
| `validate_ipc_token_len` | `commands/workflow/mod.rs` | Used only by `play` and `random` |

**Back-compat shims:** `commands/mod.rs` will re-export everything `media-control` and `media-control-daemon` currently import via `pub use` so call sites in those crates need at most mechanical path updates (or no updates at all). Story 001-002 / 001-003 may either update call sites or rely on shims; the bolt picks "rely on shims now, leave a TODO to inline call-site paths in a follow-up" to keep this bolt's diff small.

**Classification correction from inception** (worth flagging):

- **`keep.rs`** was inception-classified as window-mgmt (8 files for window/, 6 for workflow/). It's actually pure mpv-IPC (sends `script-message keep/favorite/delete/add-o`; no Hyprland touch). Moving it to `workflow/` instead. Final counts: **7 window, 7 workflow.**

This is a real correction, not a scope expansion — `keep` belongs with the other mpv-IPC commands. Documenting in the bolt's construction log.

**Execution order (per stories 001-001 → 001-003):**

1. **Scaffolding** (Story 001-001): Create empty `window/mod.rs`, `workflow/mod.rs`, `shared.rs`. Add `pub mod window; pub mod workflow; pub mod shared;` to `commands/mod.rs`. Confirm `cargo build` passes with empty modules.
2. **Move window-mgmt files** (Story 001-002): `jj mv` 7 files into `window/`. Update `mod.rs` declarations: in `commands/mod.rs` remove the per-file `pub mod` lines for moved files; add them under `commands/window/mod.rs`. Update intra-window imports (e.g., `close.rs` → `super::fullscreen::is_pip_title` still works as siblings).
3. **Move workflow files** (Story 001-003 part 1): `jj mv` 7 files into `workflow/`. Same `mod.rs` plumbing as step 2.
4. **Migrate helpers** (Story 001-003 part 2): Move helper functions from `commands/mod.rs` into their classified destinations per the table above. Update import paths in moved command files (`use super::X` becomes `use super::X` if X is in the same subnamespace, or `use crate::commands::shared::X` for shared items).
5. **Add back-compat shims** in `commands/mod.rs` so `media-control` and `media-control-daemon` keep building without source edits in those crates (or do mechanical path edits if shim count grows uncomfortable).
6. **Confirm green:** `just lint && cargo build --workspace && cargo test --workspace`.

**Subtle gotchas:**

- `commands/mod.rs` currently has 1800+ lines of inline tests at the bottom (line 1053+). These tests cover the helpers that are being moved. They migrate alongside their helpers — most go to `commands/window/mod.rs` (suppress, restore_focus, focus_window_action) or `commands/workflow/mod.rs` (mpv socket paths, validate_ipc_token_len). Test helper imports (`use super::*`, `use super::async_env_test_mutex`) update mechanically.
- `avoid.rs` tests at `:732` reference `crate::commands::clear_suppression` directly. After the move this becomes `crate::commands::window::clear_suppression`. Either update or rely on a shim.
- `mark_watched.rs` tests at `:143` reference `crate::commands::{MPV_IPC_SOCKET_DEFAULT, async_env_test_mutex}`. After the move: `crate::commands::workflow::MPV_IPC_SOCKET_DEFAULT` and `crate::commands::shared::async_env_test_mutex`. Either update or shim.
- The doc-test in `commands/mod.rs` at the top references `media_control_lib::commands::CommandContext` and `media_control_lib::commands::get_media_window`. After the move both still resolve via shims (`pub use shared::CommandContext;` and `pub use window::get_media_window;`). No doc-test edits needed.

### Acceptance Criteria

- [ ] `commands/window/` contains 7 files (avoid, fullscreen, move_window, pin, minify, focus, close) plus `mod.rs` with window-internal helpers
- [ ] `commands/workflow/` contains 7 files (mark_watched, chapter, play, seek, status, random, **keep**) plus `mod.rs` with workflow-internal mpv-IPC plumbing
- [ ] `commands/shared.rs` exists with `CommandContext`, `runtime_dir`, `async_env_test_mutex`
- [ ] `commands/mod.rs` is short: module declarations + back-compat re-exports
- [ ] `cargo build --workspace` clean, no new warnings
- [ ] `cargo test --workspace` clean, all existing tests pass
- [ ] `jj diff` shows file moves, `mod` declaration updates, import path updates — zero logic edits
- [ ] Construction log records: layout choice (Option A) and the `keep.rs` classification correction
