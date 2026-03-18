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

- [ ] **1. Plan**: List test scenarios per command <!-- tw:69facbb1-8ec1-41c5-befc-20e1c14a9afa -->
- [ ] **2. Implement**: Write tests <!-- tw:e3eee0de-4745-4805-a217-685eab6652dd -->
- [ ] **3. Verify**: All tests pass, no flakes <!-- tw:34085a00-6699-4c4e-94c2-51ed026da8d7 -->

## Dependencies

### Requires
- 001-mock-infrastructure

### Enables
- 005-logic-cleanup (fullscreen/close cleanup needs these tests)

## Success Criteria

- [ ] Move, pin, close, focus all have E2E tests <!-- tw:806eb7e1-931f-4bf8-9b67-716b969a819f -->
- [ ] Window matching, config, and suppress edge cases covered <!-- tw:d9d64dc1-6e3a-45eb-a74f-399c62f43f45 -->
- [ ] Daemon debounce logic tested <!-- tw:b2051975-28cc-4c79-9f19-b7f439c3ef26 -->
- [ ] No flaky tests <!-- tw:47368f15-0fb0-4865-8586-12b9a2f0313a -->
