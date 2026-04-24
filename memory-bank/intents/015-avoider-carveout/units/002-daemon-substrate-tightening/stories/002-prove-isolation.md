---
id: 002-prove-isolation
unit: 002-daemon-substrate-tightening
intent: 015-avoider-carveout
status: ready
priority: must
created: 2026-04-26T00:00:00Z
assigned_bolt: 026-daemon-substrate-tightening
implemented: false
---

# Story: 002-prove-isolation

## User Story

**As a** developer protecting the daemon's contract from regression
**I want** a CI-runnable verification that proves the daemon cannot import workflow code
**So that** future contributors get an immediate signal if they reach for a workflow module from the daemon

## Acceptance Criteria

- [ ] **Given** the enforcement mechanism applied in story 001, **When** I add a test/check that adds `use media_control_lib::commands::workflow::mark_watched;` to a file inside `crates/media-control-daemon/`, **Then** the build (or doctest) fails with a clear diagnostic
- [ ] **Given** the verification, **When** CI runs on every push, **Then** the verification is part of the CI suite (not a manual check)
- [ ] **Given** option 1 (cargo feature) was chosen, **When** the verification runs, **Then** it asserts (via `cargo tree` or equivalent) that `reqwest` is not in `media-control-daemon`'s resolved dependency tree

## Technical Notes

**Form depends on the option chosen in story 001:**

- **Option 1 (cargo feature):** Add a test that runs `cargo tree -p media-control-daemon --format '{p}'` and asserts the absence of `reqwest`. Could be a `tests/dep_isolation.rs` integration test that shells out, or a CI step.
- **Option 2 (`pub(crate)` + facade):** Add a `compile_fail` doctest in `crates/media-control-daemon/src/main.rs`'s top-of-file doc:
  ```rust
  //! ```compile_fail
  //! use media_control_lib::commands::workflow::mark_watched;
  //! ```
  ```
- **Option 3 (`compile_fail` doctest only):** Same as Option 2.

**CI integration**: Whatever form is chosen, ensure it runs in `.forgejo/workflows/ci.yaml` (the project uses Forgejo per global CLAUDE.md). If a new step is added, also document it in `AGENTS.md` so contributors know the signal.

## Dependencies

### Requires

- 001-pick-and-apply-enforcement

### Enables

- All stories in unit 003 (avoider-cleanup) — though they could in principle land before this story, the contract should exist before behavior changes that rely on it

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| Verification depends on `cargo tree` output format that may change between Rust versions | Pin the parsing to stable `--format` flags; fall back to grepping known-stable substrings |
| `compile_fail` doctest passes (i.e., the import doesn't fail) | CI fails loudly; the enforcement mechanism is not actually enforcing |
| Future contributor needs to add a legitimate workflow-shaped command to the daemon | They must update both the enforcement and this verification, with explicit justification in PR description |

## Out of Scope

- Choosing the enforcement mechanism (story 001)
- Adding similar enforcement for other crate boundaries
