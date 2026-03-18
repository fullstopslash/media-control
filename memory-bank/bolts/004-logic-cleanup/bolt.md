---
id: 004-logic-cleanup
unit: 003-logic-cleanup
intent: 001-test-and-refactor
type: simple-construction-bolt
status: planned
stories:
  - 001-simplify-avoid
created: 2026-03-18T13:00:00Z
started: null
completed: null
current_stage: null
stages_completed: []

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

- [ ] **1. Plan**: Design cleaner structure (enum-based case dispatch?)
- [ ] **2. Implement**: Refactor avoid.rs
- [ ] **3. Verify**: All avoid tests pass, existing tests pass

## Dependencies

### Requires
- 002-test-coverage (avoid tests must exist as safety net)

### Enables
- 005-logic-cleanup

## Success Criteria

- [ ] All avoid E2E tests pass
- [ ] No function exceeds 4 nesting levels
- [ ] Duplicate "Case 3" comment resolved
- [ ] Shared patterns extracted
