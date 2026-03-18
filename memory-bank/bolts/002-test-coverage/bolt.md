---
id: 002-test-coverage
unit: 002-test-coverage
intent: 001-test-and-refactor
type: simple-construction-bolt
status: complete
stories:
  - 001-avoid-tests
  - 002-fullscreen-tests
created: 2026-03-18T13:00:00Z
started: 2026-03-18T15:00:00Z
completed: 2026-03-18T16:15:00Z
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-18T15:15:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-18T16:00:00Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-03-18T16:15:00Z
    artifact: test-walkthrough.md

requires_bolts: [001-mock-infrastructure]
enables_bolts: [004-logic-cleanup]
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 2
  testing_scope: 3
---

# Bolt: 002-test-coverage

## Overview

E2E tests for the two most complex commands: avoid (4 cases) and fullscreen (enter/exit/retry/pin).

## Objective

Cover the highest-risk command logic with comprehensive tests. These are the commands most likely to have subtle bugs and the ones that will be refactored in unit 003.

## Stories Included

- **001-avoid-tests**: Avoid command E2E + edge cases (Must)
- **002-fullscreen-tests**: Fullscreen command E2E + edge cases (Must)

## Bolt Type

**Type**: Simple Construction Bolt
**Definition**: `.specsmd/aidlc/templates/construction/bolt-types/simple-construction-bolt.md`

## Stages

- [ ] **1. Plan**: Design test scenarios for each avoidance case and fullscreen state
- [ ] **2. Implement**: Write E2E tests using mock infrastructure
- [ ] **3. Verify**: All tests pass, no flakes

## Dependencies

### Requires
- 001-mock-infrastructure

### Enables
- 004-logic-cleanup (avoid simplification needs these tests as safety net)

## Success Criteria

- [ ] All 4 avoid cases tested end-to-end
- [ ] Fullscreen enter/exit/retry/pin-restore tested
- [ ] Edge cases from FR-3 covered for avoid and fullscreen
- [ ] No flaky tests
