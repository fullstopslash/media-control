---
id: 006-strategy-engine
unit: 001-strategy-engine
intent: 002-smart-next-episode
type: simple-construction-bolt
status: complete
stories:
  - 001-config-types
  - 002-strategy-dispatch
created: 2026-03-18T20:00:00Z
started: 2026-03-18T20:30:00Z
completed: 2026-03-18T21:15:00Z
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-18T20:30:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-18T21:00:00Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-03-18T21:15:00Z
    artifact: test-walkthrough.md

requires_bolts: []
enables_bolts: [008-strategy-implementations]
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

# Bolt: 006-strategy-engine

## Overview

Config types, strategy enum, rule matching, and integration with mark-watched-and-next.

## Stories Included

- **001-config-types**: Config types and TOML parsing (Must)
- **002-strategy-dispatch**: Strategy dispatch and integration (Must)

## Bolt Type

**Type**: Simple Construction Bolt

## Stages

- [ ] **1. Plan**: Design config format, strategy trait, dispatch flow <!-- tw:d73e7455-09ee-4d4e-8a6b-a0ddbd8eddb8 -->
- [ ] **2. Implement**: Config types, rule matching, integration point <!-- tw:f18e6d2f-ffdc-4a13-bdb7-f56b36c39036 -->
- [ ] **3. Verify**: Config parsing tests, dispatch logic tests <!-- tw:8c556f80-e92b-40d8-a29b-87b388dd64f6 -->

## Dependencies

### Requires
- None (foundation bolt)

### Enables
- 008-strategy-implementations
