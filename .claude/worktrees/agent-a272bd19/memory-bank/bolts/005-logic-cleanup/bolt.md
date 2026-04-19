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

- [ ] **1. Plan**: Identify specific changes for each file <!-- tw:fd831833-fa78-41be-8dfc-47e3daa4af72 -->
- [ ] **2. Implement**: Refactor fullscreen.rs, close.rs, error audit <!-- tw:379d4f03-1860-4aef-a55a-ff83e5d89fd8 -->
- [ ] **3. Verify**: All tests pass, final audit clean <!-- tw:73cc0249-eadf-405a-9e64-ddff33647071 -->

## Dependencies

### Requires
- 003-test-coverage (fullscreen/close tests must exist)
- 004-logic-cleanup (avoid cleanup done first)

### Enables
- None (final bolt)

## Success Criteria

- [ ] All tests pass (existing + new) <!-- tw:910b80ab-1aff-4cb2-a790-34bad4ad861a -->
- [ ] `_clients` param removed from exit_fullscreen <!-- tw:59ef3a37-1654-4c2f-b7b7-7e132b6a70a1 -->
- [ ] `#[allow(clippy::too_many_arguments)]` removed <!-- tw:7c187d38-e426-4a32-9f57-cbf4c5914df7 -->
- [ ] Close has single killwindow path for non-mpv/non-PiP <!-- tw:07110496-67e4-444e-b095-83a7e61edf39 -->
- [ ] No remaining verbose .map_err() patterns <!-- tw:3ccddcf4-47d4-4455-b94d-0084073aa08e -->
- [ ] chapter.rs error variant is semantically correct <!-- tw:49700fc4-d5eb-4f48-a3f8-ae701e4e0d51 -->
