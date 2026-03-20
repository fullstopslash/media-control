---
id: 013-daemon-signals
unit: 001-daemon-signals
intent: 008-daemon-reliability
type: simple-construction-bolt
status: complete
stories:
  - 001-sigterm-handling
created: 2026-03-19T00:00:00.000Z
started: 2026-03-19T00:00:00.000Z
completed: "2026-03-20T04:33:10Z"
current_stage: null
stages_completed: []
requires_bolts: []
enables_bolts: []
requires_units: []
blocks: false
complexity:
  avg_complexity: 1
  avg_uncertainty: 1
  max_dependencies: 0
  testing_scope: 1
---

# Bolt: 013-daemon-signals

## Overview

Add SIGTERM handling to the daemon's foreground select! loop for clean shutdown.

## Objective

Enable `media-control-daemon stop` to trigger a clean daemon shutdown with proper resource cleanup.

## Stories Included

- **001-sigterm-handling**: Handle SIGTERM for clean daemon shutdown (Must)

## Bolt Type

**Type**: Simple Construction Bolt

## Stages

- [ ] **1. Plan**: Implementation plan <!-- tw:8987b552-46a2-43f2-ade2-d1c94797bc90 -->
- [ ] **2. Implement**: Code changes <!-- tw:e78ff2a2-96f0-48b7-8f4d-1a70244819f9 -->
- [ ] **3. Test**: Verification <!-- tw:d7f2f37e-300a-41a3-a0d4-8831aabb888a -->

## Dependencies

### Requires
- None

### Enables
- None

## Success Criteria

- [ ] SIGTERM branch added to foreground select! <!-- tw:60eb48c5-c318-4858-9c4e-7df1259439a4 -->
- [ ] `cargo check` passes <!-- tw:e25d122d-9670-4805-8cb0-887ff821cab5 -->
- [ ] `cargo clippy` passes <!-- tw:b52024de-ea03-44af-bed0-c89c5f239417 -->
- [ ] `cargo test` passes <!-- tw:ab3f3b73-1d56-4b27-bdb9-814d80cf7af8 -->
