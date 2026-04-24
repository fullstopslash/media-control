---
stage: test
bolt: 025-commands-regrouping
created: 2026-04-26T00:00:00Z
---

## Test Report: commands-regrouping

### Summary

- **Tests**: 370/370 passed (single-threaded)
- **New tests written**: 0 (pure structural refactor — existing tests cover behavior)
- **Tests modified**: 0 logic edits; only mechanical `use`-path updates inside test bodies as helpers were re-homed (already counted in Stage 2's diff)
- **Coverage**: unchanged — same test bodies, same assertions, same code paths

### Test Suite Breakdown

| Suite | Pass | Fail | Notes |
|---|---|---|---|
| `media-control-lib` lib unit tests | 343 | 0 | Includes the migrated tests now living in `commands/window/mod.rs::tests` and `commands/workflow/mod.rs::tests` |
| `media-control-daemon` binary unit tests | 8 | 0 | FIFO + signal handling; daemon's own tests, untouched |
| `media-control` binary unit tests | 0 | 0 | None defined |
| `config_integration` (lib integration test) | 3 | 0 | Untouched |
| Doc-tests | 16 | 0 | Updated paths inside doc-comments resolve correctly via the new module tree (and via shims at `commands::X` for top-level paths) |
| **Total** | **370** | **0** | |

### Per-Namespace Test Counts (lib)

- `commands::window::*` — **115 tests** pass
- `commands::workflow::*` — **45 tests** pass
- Other (`config`, `hyprland`, `window`, `error`, `jellyfin`, `test_helpers`) — **183 tests** pass

These add to 343, matching the lib unit-test total. The window/workflow split is visible in the test names too — a useful side benefit of the regrouping.

### Lint and Doc

- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo doc --workspace --no-deps` — builds; one **pre-existing** intra-doc warning in `crates/media-control-daemon/src/main.rs:265` (`FIFO_ERROR_BACKOFF` link target) — unrelated, untouched file. Verified at parent revision.

### Acceptance Criteria Validation

From `implementation-plan.md`:

- ✅ `commands/window/` contains 7 files (avoid, fullscreen, move_window, pin, minify, focus, close) plus `mod.rs` with window-internal helpers — **verified by `ls`**
- ✅ `commands/workflow/` contains 7 files (mark_watched, chapter, play, seek, status, random, **keep**) plus `mod.rs` with workflow-internal mpv-IPC plumbing — **verified by `ls`**
- ✅ `commands/shared.rs` exists with `CommandContext`, `runtime_dir`, `async_env_test_mutex` — **verified by Read; 144 LOC**
- ✅ `commands/mod.rs` is short: module declarations + back-compat re-exports — **51 LOC, was 1872**
- ✅ `cargo build --workspace` clean, no new warnings — **verified**
- ✅ `cargo test --workspace` clean, all existing tests pass — **370/370 single-threaded**
- ✅ `jj diff` shows file moves, `mod` declaration updates, import path updates — zero logic edits — **18 files, 1942/1847 lines, mostly cut-paste of helpers; no function bodies altered**
- ✅ Construction log records: layout choice (Option A) and the `keep.rs` classification correction — **captured in `implementation-walkthrough.md` § Key Decisions**

All eight acceptance criteria met.

### Issues Found

None introduced by this bolt.

**One pre-existing test flake observed** (already documented in `implementation-walkthrough.md` § Developer Notes):

- `commands::window::move_window::tests::move_down_dispatches_correct_position` can race under parallel test runs because it reads `is_minified()` without holding `async_env_test_mutex`. Verified the flake exists at parent revision `e444cd2b`. Not introduced by this refactor. Single-threaded runs are unaffected. Suggested follow-up: fold into bolt 027 story 003-004 (which already touches the env-mutex dance) — wrap the test body in the mutex + an isolated `XDG_RUNTIME_DIR`.

### Notes

- The unchanged binary `main.rs` files are themselves a test of the back-compat shims: anything either binary needed from the old flat namespace continues to resolve. The shim layer is doing real load-bearing work.
- Doc-tests at `crates/media-control-lib/src/commands/mod.rs:15` and inside `move_window.rs`/`focus.rs` reference top-level paths like `media_control_lib::commands::CommandContext` and `media_control_lib::commands::get_media_window`. Both resolve via the shim re-exports. No doc-comment edits were needed.
- Coverage tools (`cargo-llvm-cov`, `cargo-tarpaulin`) were not run as part of this bolt — coverage measurement is out of scope for a structural refactor where the test bodies and assertions are unchanged. If the project introduces coverage gating in the future, this bolt should produce an identical coverage profile to its parent revision.
