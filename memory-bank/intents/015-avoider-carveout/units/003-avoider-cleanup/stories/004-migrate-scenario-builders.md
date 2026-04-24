---
id: 004-migrate-scenario-builders
unit: 003-avoider-cleanup
intent: 015-avoider-carveout
status: ready
priority: must
created: 2026-04-26T00:00:00Z
assigned_bolt: 027-avoider-cleanup
implemented: false
---

# Story: 004-migrate-scenario-builders

## User Story

**As a** maintainer adding new avoidance test cases (now or in the future)
**I want** the scenario-construction boilerplate (long `make_test_client_full(...)` arg lists, `scenario_single_workspace`, the env-var save/restore + tokio mutex dance) extracted into reusable builders in `test_helpers.rs`
**So that** new tests are short and readable, the inline-test section of `avoid.rs` shrinks substantially, and any future daemon test (e.g., for cached-clients in story 005) gets the same primitives without copy/paste

## Acceptance Criteria

- [ ] **Given** the repeated `make_test_client_full(...)` blocks at avoid.rs:888-915, 982-1009, 1035-1062, 1083-1110, 1133-1160, 1221-1260 (12 positional args each), **When** I introduce `ClientBuilder` in `test_helpers.rs` (e.g., `ClientBuilder::new(addr, class).with_position(x, y, w, h).focused().build()`), **Then** the test bodies in `avoid.rs` use the builder and read in 2-3 lines per client instead of 12-positional-arg invocations
- [ ] **Given** `scenario_single_workspace` (avoid.rs:851-881), **When** I move it to `test_helpers.rs`, **Then** it's available to any test (including future daemon tests) and the original location has only `use` updates
- [ ] **Given** `assert_handler_warms_suppression` env-mutex dance (avoid.rs:2031-2088, called from avoid.rs:2090, 2127), **When** I extract `with_isolated_runtime_dir(async fn) -> R` (or equivalent primitive) into `test_helpers.rs`, **Then** the unsafe env-var save/restore + tokio mutex coordination lives in one place

## Technical Notes

- `ClientBuilder` should match today's `Client` shape exactly; this is purely an ergonomic wrapper around the existing constructor
- `with_isolated_runtime_dir` must keep the same safety properties: tokio mutex prevents test cross-talk on the global env var; restore is panic-safe (use a guard struct with `Drop`)
- Per FR-5, do not create a parallel `mod test_helpers` in the daemon crate; daemon tests `use media_control_lib::test_helpers::...`
- This story may grow `test_helpers.rs` substantially — that's the point; concentration of test infrastructure is the goal

## Dependencies

### Requires

- 003-collapse-classify-dispatch-and-restore-focus-helper

### Enables

- 005-daemon-cached-clients (the new daemon tests need these primitives)
- 006-daemon-in-memory-suppress (same)

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| `ClientBuilder` setters return `Self` (consuming) vs. `&mut Self` (chaining) | Consuming `Self` matches Rust idioms; chaining stays ergonomic |
| A test needs a client variant `ClientBuilder` doesn't cover | Add the setter; don't fall back to direct `Client { ... }` literals (defeats the purpose) |
| `with_isolated_runtime_dir` is reused by daemon tests in a different crate | Public API; documented with a doc-test or example |
| The migration leaves dead helpers in `avoid.rs` tests | Remove them — `cargo clippy --workspace -- -D warnings` catches `dead_code` |

## Out of Scope

- Touching production code in `avoid.rs` (covered by stories 001-003)
- Adding new test cases (this story is purely structural)
