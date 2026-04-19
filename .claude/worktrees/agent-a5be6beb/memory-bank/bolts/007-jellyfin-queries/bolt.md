---
id: 007-jellyfin-queries
unit: 002-jellyfin-queries
intent: 002-smart-next-episode
type: simple-construction-bolt
status: complete
stories:
  - 001-library-detection
  - 002-item-queries
created: 2026-03-18T20:00:00Z
started: 2026-03-18T21:15:00Z
completed: 2026-03-18T21:30:00Z
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-18T21:15:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-18T21:30:00Z
  - name: test
    completed: 2026-03-18T21:30:00Z

requires_bolts: []
enables_bolts: [008-strategy-implementations]
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 2
  testing_scope: 2
---

# Bolt: 007-jellyfin-queries

## Overview

New JellyfinClient methods: library detection via Ancestors, filtered item queries, collection items.

## Stories Included

- **001-library-detection**: Library detection via Ancestors API (Must)
- **002-item-queries**: Filtered item queries for strategies (Must)

## Bolt Type

**Type**: Simple Construction Bolt

## Stages

- [ ] **1. Plan**: Design API methods and response types <!-- tw:7fab500c-25cc-48e3-978b-d5da035ddaca -->
- [ ] **2. Implement**: JellyfinClient methods, response structs <!-- tw:de62685d-4b8c-4123-9f6e-111696a8aee3 -->
- [ ] **3. Verify**: Deserialization tests with sample JSON <!-- tw:9a870199-03bc-46bb-9d7d-9c96fe8d8dab -->

## Dependencies

### Requires
- None (parallel with 006)

### Enables
- 008-strategy-implementations
