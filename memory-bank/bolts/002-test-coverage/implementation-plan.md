---
stage: plan
bolt: 002-test-coverage
created: 2026-03-18T15:00:00Z
---

## Implementation Plan: avoid + fullscreen E2E tests

### Objective
Write comprehensive E2E tests for the two most complex commands using the mock infrastructure from bolt 001.

### Deliverables
- `crates/media-control-lib/tests/avoid_e2e.rs` - Integration test file for avoid command
- `crates/media-control-lib/tests/fullscreen_e2e.rs` - Integration test file for fullscreen command

### Dependencies
- `test_helpers` module from bolt 001 (MockHyprland, test context, JSON helpers)
- `tempfile` (already dev-dependency) for suppress file tests

### Technical Approach

**Test file location**: Integration tests in `tests/` directory. This gives access to the crate's public API including `#[cfg(test)]` items like `CommandContext::for_test`.

Wait - `#[cfg(test)]` items in the lib crate are NOT visible to integration tests (integration tests are separate crates). Need to rethink.

**Revised approach**: Put E2E tests inside the lib crate as `#[cfg(test)]` modules within the command files themselves (or in dedicated test submodules). This way they can access `for_test` and `test_helpers`.

**Structure**:
- Add E2E test sections to `commands/avoid.rs` tests module
- Add E2E test sections to `commands/fullscreen.rs` tests module
- The mock server + test context are in `test_helpers.rs` (already `#[cfg(test)]`)

**Key challenge - avoid's suppress file**: The avoid command reads/writes a suppress file at `$XDG_RUNTIME_DIR/media-avoider-suppress`. Tests need to:
1. Set `XDG_RUNTIME_DIR` to a temp dir
2. Or clear the suppress file before each test
3. Be careful about env var mutation in parallel tests

**Strategy for suppress**: Each test should write "0" to the suppress file before running avoid, ensuring it's not suppressed. Use a unique temp dir per test via the `tempfile` crate.

**Fullscreen challenge - retry loop**: The retry loop calls `get_clients` multiple times. The mock needs to return different responses on successive calls to simulate "still fullscreen" → "exited fullscreen".

**Strategy for retries**: Enhance MockHyprland to support response sequences (return different responses for the same command on successive calls). Add a `set_response_sequence("j/clients", vec![json1, json2])` method.

### Avoid Test Scenarios

1. **Case 1 - single workspace, non-media focused, media not at primary**: moves media to primary
2. **Case 1 - media already at primary**: no move dispatched
3. **Case 1 - focused is fullscreen non-media**: early return (no move)
4. **Case 1 - no media windows**: early return
5. **Case 2 - mouseover, at primary**: toggles to secondary
6. **Case 2 - mouseover, at secondary**: toggles to primary
7. **Case 2 - mouseover, no previous window**: early return
8. **Case 3 - multi-workspace, media focused, overlap**: geometry-based move + focus restore
9. **Case 3 - multi-workspace, non-media focused, overlap**: geometry-based move
10. **Case 3 - no overlap**: no move
11. **Case 4 - fullscreen non-media**: moves media away
12. **Edge - no focused window**: early return
13. **Edge - suppressed**: early return
14. **Edge - position override by class**: applies override positions

### Fullscreen Test Scenarios

1. **Enter fullscreen - unpinned**: focus + fullscreen batch
2. **Enter fullscreen - pinned**: focus + unpin + fullscreen batch
3. **Exit fullscreen - basic**: exits, repositions, restores focus
4. **Exit fullscreen - pin restore**: exits, re-pins, repositions
5. **Auto-pin - always_pin set, not pinned, not floating**: floats then pins
6. **Auto-pin - always_pin set, not pinned, already floating**: just pins
7. **No media window**: silent no-op
8. **PiP title triggers pin restore**: "Picture-in-Picture" title → pin restored on exit

### Acceptance Criteria
- [ ] All avoid cases (1-4) have at least one E2E test
- [ ] All avoid edge cases tested
- [ ] Fullscreen enter/exit/retry/auto-pin/PiP all tested
- [ ] No tests depend on env var state from other tests
- [ ] No flaky tests
