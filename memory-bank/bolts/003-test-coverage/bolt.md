---
id: 003-test-coverage
unit: 002-test-coverage
intent: 001-test-and-refactor
type: simple-construction-bolt
status: complete
stories:
  - 003-simple-command-tests
  - 004-edge-cases
  - 005-daemon-tests
created: 2026-03-18T13:00:00Z
started: 2026-03-18T16:30:00Z
completed: 2026-03-18T17:30:00Z
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-18T16:30:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-18T17:00:00Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-03-18T17:30:00Z
    artifact: test-walkthrough.md

requires_bolts: [001-mock-infrastructure]
enables_bolts: [005-logic-cleanup]
requires_units: []
blocks: false

complexity:
  avg_complexity: 1
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

# Bolt: 003-test-coverage

## Overview

E2E tests for simpler commands (move, pin, close, focus), cross-cutting edge cases, and daemon robustness.

## Objective

Complete the test coverage for all remaining commands and edge cases. These tests are lower-risk individually but collectively ensure no blind spots.

## Stories Included

- **003-simple-command-tests**: Move, pin, close, focus E2E (Must)
- **004-edge-cases**: Cross-cutting edge cases (Should)
- **005-daemon-tests**: Daemon debounce and lifecycle (Should)

## Bolt Type

**Type**: Simple Construction Bolt
**Definition**: `.specsmd/aidlc/templates/construction/bolt-types/simple-construction-bolt.md`

## Stages

- [ ] **1. Plan**: List test scenarios per command
- [ ] **2. Implement**: Write tests
- [ ] **3. Verify**: All tests pass, no flakes

## Dependencies

### Requires
- 001-mock-infrastructure

### Enables
- 005-logic-cleanup (fullscreen/close cleanup needs these tests)

## Success Criteria

- [ ] Move, pin, close, focus all have E2E tests
- [ ] Window matching, config, and suppress edge cases covered
- [ ] Daemon debounce logic tested
- [ ] No flaky tests
