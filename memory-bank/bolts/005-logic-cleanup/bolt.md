---
id: 005-logic-cleanup
unit: 003-logic-cleanup
intent: 001-test-and-refactor
type: simple-construction-bolt
status: complete
stories:
  - 002-simplify-fullscreen-close
  - 003-error-consistency
created: 2026-03-18T13:00:00Z
started: 2026-03-18T19:00:00Z
completed: 2026-03-18T19:45:00Z
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-18T19:00:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-18T19:30:00Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-03-18T19:45:00Z
    artifact: test-walkthrough.md

requires_bolts: [003-test-coverage, 004-logic-cleanup]
enables_bolts: []
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

# Bolt: 005-logic-cleanup

## Overview

Clean up fullscreen, close, and error handling - the remaining refactoring targets.

## Objective

Simplify exit_fullscreen (remove unused param, flatten retry), deduplicate close's killwindow branches, and do a final error consistency audit.

## Stories Included

- **002-simplify-fullscreen-close**: Simplify fullscreen and close commands (Must)
- **003-error-consistency**: Error handling consistency pass (Should)

## Bolt Type

**Type**: Simple Construction Bolt
**Definition**: `.specsmd/aidlc/templates/construction/bolt-types/simple-construction-bolt.md`

## Stages

- [ ] **1. Plan**: Identify specific changes for each file
- [ ] **2. Implement**: Refactor fullscreen.rs, close.rs, error audit
- [ ] **3. Verify**: All tests pass, final audit clean

## Dependencies

### Requires
- 003-test-coverage (fullscreen/close tests must exist)
- 004-logic-cleanup (avoid cleanup done first)

### Enables
- None (final bolt)

## Success Criteria

- [ ] All tests pass (existing + new)
- [ ] `_clients` param removed from exit_fullscreen
- [ ] `#[allow(clippy::too_many_arguments)]` removed
- [ ] Close has single killwindow path for non-mpv/non-PiP
- [ ] No remaining verbose .map_err() patterns
- [ ] chapter.rs error variant is semantically correct
