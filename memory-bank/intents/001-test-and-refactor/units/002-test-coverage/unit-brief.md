---
unit: 002-test-coverage
intent: 001-test-and-refactor
phase: inception
status: ready
created: 2026-03-18T13:00:00Z
updated: 2026-03-18T13:00:00Z
unit_type: cli
default_bolt_type: simple-construction-bolt
---

# Unit Brief: Test Coverage

## Purpose

Write comprehensive end-to-end tests for every command, edge case tests for boundary conditions, and daemon robustness tests. These tests serve as both verification and a safety net for the subsequent refactoring unit.

## Scope

### In Scope
- E2E tests for all 8 command modules (fullscreen, move, close, focus, avoid, pin, chapter, mark-watched)
- Edge case tests for avoidance logic, fullscreen retry, window matching
- Daemon debounce and lifecycle tests
- Suppress file timing tests

### Out of Scope
- Jellyfin HTTP integration tests (existing deserialization tests are sufficient)
- mpv IPC integration tests
- playerctl integration tests
- UI/visual testing

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-2 | End-to-End Command Tests | Must |
| FR-3 | Edge Case Coverage | Must |
| FR-6 | Daemon Robustness | Should |

---

## Domain Concepts

### Key Operations
| Operation | Description | Inputs | Outputs |
|-----------|-------------|--------|---------|
| test_command_e2e | Run a command against mock, verify dispatched commands | Mock setup, CommandContext, command call | Assertion pass/fail |
| test_edge_case | Exercise boundary condition | Specific mock state | Assertion pass/fail |
| test_daemon_behavior | Verify event loop properties | Event sequence | Assertion pass/fail |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 5 |
| Must Have | 3 |
| Should Have | 2 |
| Could Have | 0 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-avoid-tests | Avoid command E2E + edge cases | Must | Planned |
| 002-fullscreen-tests | Fullscreen command E2E + edge cases | Must | Planned |
| 003-simple-command-tests | Move, pin, close, focus E2E tests | Must | Planned |
| 004-edge-cases | Cross-cutting edge cases (matching, config, suppress) | Should | Planned |
| 005-daemon-tests | Daemon debounce and lifecycle tests | Should | Planned |

---

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| 001-mock-infrastructure | Needs mock server and test context |

### Depended By
| Unit | Reason |
|------|--------|
| 003-logic-cleanup | Tests serve as safety net for refactoring |

---

## Constraints

- Tests must not be flaky (no timing-dependent assertions without tolerance)
- Tests must run in parallel (no shared state between tests)

---

## Success Criteria

### Functional
- [ ] Every command has at least one happy-path E2E test
- [ ] Every command handles "no media window" gracefully
- [ ] All 4 avoid cases have dedicated tests
- [ ] Fullscreen enter/exit/retry/pin-restore all tested
- [ ] Edge cases from FR-3 all covered

### Quality
- [ ] All new tests pass reliably (no flakes in 10 consecutive runs)
- [ ] Existing 118 tests still pass
