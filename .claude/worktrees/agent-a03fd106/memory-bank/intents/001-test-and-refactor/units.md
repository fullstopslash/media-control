---
intent: 001-test-and-refactor
phase: inception
status: units-decomposed
updated: 2026-03-18T13:00:00Z
---

# Test and Refactor - Unit Decomposition

## Units Overview

This intent decomposes into 3 units of work:

### Unit 1: 001-mock-infrastructure

**Description**: Build the mock Hyprland IPC server and test harness utilities that all other tests depend on.

**Assigned Requirements**: FR-1
**Deliverables**: Mock socket server module, test helper extensions, `CommandContext` test constructor
**Dependencies**: None
**Estimated Complexity**: M

### Unit 2: 002-test-coverage

**Description**: Write end-to-end command tests and edge case tests using the mock infrastructure.

**Assigned Requirements**: FR-2, FR-3, FR-6
**Deliverables**: E2E tests for all commands, edge case tests, daemon robustness tests
**Dependencies**: 001-mock-infrastructure
**Estimated Complexity**: L

### Unit 3: 003-logic-cleanup

**Description**: Refactor command implementations for clarity. Simplify avoid, fullscreen, close, and error handling. Tests from unit 2 serve as safety net.

**Assigned Requirements**: FR-4, FR-5
**Deliverables**: Cleaned up command implementations, consistent error handling
**Dependencies**: 001-mock-infrastructure, 002-test-coverage
**Estimated Complexity**: M

## Requirement-to-Unit Mapping

- **FR-1**: Mock Hyprland IPC Infrastructure → `001-mock-infrastructure`
- **FR-2**: End-to-End Command Tests → `002-test-coverage`
- **FR-3**: Edge Case Coverage → `002-test-coverage`
- **FR-4**: Logic Cleanup and Simplification → `003-logic-cleanup`
- **FR-5**: Error Handling Consistency → `003-logic-cleanup`
- **FR-6**: Daemon Robustness → `002-test-coverage`

## Unit Dependency Graph

```text
[001-mock-infrastructure] ──► [002-test-coverage] ──► [003-logic-cleanup]
```

## Execution Order

1. Unit 001: Mock infrastructure (foundation)
2. Unit 002: Test coverage (uses mocks, creates safety net)
3. Unit 003: Logic cleanup (refactor with confidence)
