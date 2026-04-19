---
id: 008-strategy-implementations
unit: 003-strategy-implementations
intent: 002-smart-next-episode
type: simple-construction-bolt
status: in-progress
stories:
  - 001-next-up-and-random
  - 002-recent-unwatched
  - 003-series-or-random
created: 2026-03-18T20:00:00Z
started: 2026-03-18T21:30:00Z
completed: null
current_stage: implement
stages_completed:
  - name: plan
    completed: 2026-03-18T21:30:00Z

requires_bolts: [006-strategy-engine, 007-jellyfin-queries]
enables_bolts: []
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 2
  max_dependencies: 2
  testing_scope: 2
---

# Bolt: 008-strategy-implementations

## Overview

Implement all 4 strategies: next-up, recent-unwatched, series-or-random, random-unwatched.

## Stories Included

- **001-next-up-and-random**: next-up and random-unwatched (Must)
- **002-recent-unwatched**: recent-unwatched strategy (Must)
- **003-series-or-random**: series-or-random strategy (Must)

## Bolt Type

**Type**: Simple Construction Bolt

## Stages

- [ ] **1. Plan**: Design each strategy's logic flow <!-- tw:c7224e07-de26-4368-8e38-6a2da7bbc851 -->
- [ ] **2. Implement**: All 4 strategies + end-to-end test with real Jellyfin <!-- tw:cd238719-d38b-4818-9736-aa6825a7b005 -->
- [ ] **3. Verify**: Unit tests, manual test with each library type <!-- tw:5e81ffba-89bd-4e4c-aa95-9168ef07d69d -->

## Dependencies

### Requires
- 006-strategy-engine (config + dispatch)
- 007-jellyfin-queries (API methods)

### Enables
- None (final bolt)
