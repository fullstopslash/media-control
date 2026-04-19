---
id: 004-logic-cleanup
unit: 003-logic-cleanup
intent: 001-test-and-refactor
type: simple-construction-bolt
status: complete
stories:
  - 001-simplify-avoid
created: 2026-03-18T13:00:00Z
started: 2026-03-18T18:00:00Z
completed: 2026-03-18T18:45:00Z
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-18T18:00:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-18T18:30:00Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-03-18T18:45:00Z
    artifact: test-walkthrough.md

requires_bolts: [002-test-coverage]
enables_bolts: [005-logic-cleanup]
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 3
---

# Bolt: 004-logic-cleanup

## Overview

Refactor the avoid command - the most complex and most impactful cleanup target.

## Objective

Restructure avoid.rs to reduce nesting, extract shared patterns, and make the 4 avoidance cases clearly separated. All avoid E2E tests must continue to pass.

## Stories Included

- **001-simplify-avoid**: Simplify avoid command logic (Must)

## Bolt Type

**Type**: Simple Construction Bolt
**Definition**: `.specsmd/aidlc/templates/construction/bolt-types/simple-construction-bolt.md`

## Stages

- [ ] **1. Plan**: Design cleaner structure (enum-based case dispatch?) <!-- tw:21d5187e-82ff-438c-854e-d3600f91bbc9 -->
- [ ] **2. Implement**: Refactor avoid.rs <!-- tw:e7c22c79-e06d-4071-9843-9c231c01fb91 -->
- [ ] **3. Verify**: All avoid tests pass, existing tests pass <!-- tw:810f7681-0b31-4717-95ab-b0bcaa08869d -->

## Dependencies

### Requires
- 002-test-coverage (avoid tests must exist as safety net)

### Enables
- 005-logic-cleanup

## Success Criteria

- [ ] All avoid E2E tests pass <!-- tw:030fe7b9-89d3-46a5-bcfa-cb815541c9ba -->
- [ ] No function exceeds 4 nesting levels <!-- tw:7a7e6f5c-8454-48ac-bdc2-304603318f2d -->
- [ ] Duplicate "Case 3" comment resolved <!-- tw:dcffd37b-e848-41fc-8f0a-e5eb794f34c6 -->
- [ ] Shared patterns extracted <!-- tw:764a6ac1-1955-4ebb-8382-e4a4ffde8bab -->
