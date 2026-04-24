---
stage: implement
bolt: 025-commands-regrouping
created: 2026-04-26T00:00:00Z
---

## Implementation Walkthrough: commands-regrouping

### Summary

Reorganized `crates/media-control-lib/src/commands/` from a flat 14-file namespace + 1872-line `mod.rs` into three subnamespaces: `commands/window/` (7 files + window-internal helpers), `commands/workflow/` (7 files + mpv-IPC plumbing), and `commands/shared.rs` (3 dual-use items). `commands/mod.rs` is now 51 lines of `pub use` re-exports for back-compat. Binary entrypoints required zero edits.

### Structure Overview

The boundary that the rest of intent 015 will lean on now exists in code: `commands::window` is everything the avoider daemon legitimately needs; `commands::workflow` is everything CLI-only; `commands::shared` is the small set both groups touch (`CommandContext`, `runtime_dir`, the test mutex). The pre-existing top-level `commands::X` paths still resolve via shims, so no behavior changed and no caller had to learn a new path.

### Completed Work

- [x] `crates/media-control-lib/src/commands/mod.rs` — shrunk from 1872 to 51 lines; now re-exports only
- [x] `crates/media-control-lib/src/commands/shared.rs` — new (144 LOC); `CommandContext`, `runtime_dir`, `async_env_test_mutex`
- [x] `crates/media-control-lib/src/commands/window/mod.rs` — new (1098 LOC); window-mgmt helpers + tests
- [x] `crates/media-control-lib/src/commands/window/{avoid,close,focus,fullscreen,minify,move_window,pin}.rs` — `jj`-renamed from parent
- [x] `crates/media-control-lib/src/commands/workflow/mod.rs` — new (660 LOC); mpv-IPC plumbing + tests
- [x] `crates/media-control-lib/src/commands/workflow/{chapter,keep,mark_watched,play,random,seek,status}.rs` — `jj`-renamed from parent
- [x] `crates/media-control/src/main.rs` — unchanged (back-compat shims covered every reference)
- [x] `crates/media-control-daemon/src/main.rs` — unchanged

### Key Decisions

- **`keep.rs` reclassified to workflow**, not window-mgmt as the inception had it. `keep.rs` sends `script-message keep/favorite/delete/add-o` to mpv with no Hyprland touch — pure mpv-IPC. Final counts: 7 window, 7 workflow.
- **Layout: directory submodules** (Option A from the plan), not single-file submodules. `jj` tracks renames automatically so blame survives.
- **Back-compat shims over call-site updates.** `commands/mod.rs` re-exports the previous flat-namespace symbols via `pub use shared::*; pub use window::*; pub use workflow::*;`-style declarations. Result: zero edits to either binary's `main.rs`. Trade-off accepted: the shims stay around as a soft layer that follow-up work can chip away at; for this bolt's purpose (zero-behavior-change refactor) they are the right call.
- **Test helpers (`unsafe fn set_env`, `unsafe fn remove_env`) duplicated** to both `window/mod.rs::tests` and `workflow/mod.rs::tests` blocks rather than extracted to `crate::test_helpers`. Matches the existing pattern (per-command files like `close.rs` and `mark_watched.rs` already define these locally) and produces less churn. Story 003-004 in unit 003 will revisit this when migrating other test infrastructure.
- **One legitimate cross-namespace import**: `window/close.rs` needs `MPV_IPC_SOCKET_DEFAULT` (renamed `as SHIM_SOCKET`) and `send_to_mpv_socket` from `workflow/`. Resolved with absolute `crate::commands::workflow::{...}` paths. This is the only window→workflow boundary cross in the codebase. Worth flagging because unit 002 (substrate-tightening) will need to decide whether `close.rs` belongs in window/ at all, given it depends on workflow infrastructure to gracefully shut mpv down. For now: stays in window/ because closing windows is a window-management concern; the mpv-shutdown is a downstream side-effect.

### Deviations from Plan

- The plan suggested either inlining call-site updates or shimming. Chose shimming exclusively — produces zero edits to the two binaries, which keeps the diff focused and reviewable. (Plan-compatible.)
- Discovered the `window/close.rs` → `workflow/` cross-cut, which the plan had not predicted. Resolved via absolute path; no scope expansion. (Plan-compatible; surprise documented.)

### Dependencies Added

None. No `Cargo.toml` edits.

### Verification

- [x] `cargo build --workspace` — clean
- [x] `cargo clippy --workspace --all-targets -- -D warnings` — clean
- [x] `cargo test --workspace -- --test-threads=1` — 370 tests pass (343 lib + 8 + 3 + 16 doc-tests)
- [x] `cargo doc --workspace --no-deps` — builds; one pre-existing intra-doc warning in `media-control-daemon/src/main.rs:265` (unrelated, untouched file)
- [x] `jj diff --stat` — 18 files changed, 1942 insertions / 1847 deletions; almost all from helper migrations (mostly cuts from `commands/mod.rs` and pastes into the new subnamespace `mod.rs` files)

### Developer Notes

- **Pre-existing test flake** (NOT caused by this refactor): `commands::window::move_window::tests::move_down_dispatches_correct_position` can race under parallel test runs because it reads `is_minified()` without holding `async_env_test_mutex`, so a sibling `minify` test's `XDG_RUNTIME_DIR` swap can occasionally race it. Verified the flake exists at the parent revision (`e444cd2b`) too. Use `--test-threads=1` for now; consider wrapping the test in the env mutex in a follow-up cleanup (could fold into bolt 027 story 003-004 since that story already touches the env-mutex dance).
- **Shim drift hazard.** The `pub use` shims in `commands/mod.rs` will silently keep working as new code is added — there's nothing today preventing a daemon contributor from importing, e.g., `media_control_lib::commands::send_mpv_script_message` (which would resolve via the shim through `commands::workflow::send_mpv_script_message`). That enforcement is bolt 026's job. Until 026 lands, the boundary is structural-but-not-enforced.
