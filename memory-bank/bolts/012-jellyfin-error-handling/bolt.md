---
id: 012-jellyfin-error-handling
unit: 001-jellyfin-error-handling
intent: 007-jellyfin-error-handling
type: simple-construction-bolt
status: complete
stories:
  - 001-get-error-status
  - 002-resume-error-logging
created: 2026-03-19T20:00:00.000Z
started: 2026-03-19T20:00:00.000Z
completed: "2026-03-20T04:33:37Z"
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

# Bolt: 012-jellyfin-error-handling

## Overview

Add `.error_for_status()?` to all Jellyfin GET requests and log resume position errors in play.rs.

## Objective

Surface HTTP error status codes instead of confusing JSON parse errors, and make resume position failures visible.

## Stories Included

- **001-get-error-status**: Add error_for_status to GET requests (Must)
- **002-resume-error-logging**: Log resume ticks errors in play.rs (Must)

## Bolt Type

**Type**: Simple Construction Bolt

## Stages

- [ ] **1. Plan**: Implementation plan → implementation-plan.md <!-- tw:6e9a6892-541d-47a8-bb0e-8ce4259080ed -->
- [ ] **2. Implement**: Code changes → modified source files <!-- tw:c1487b47-9a32-4acc-8f84-0af28bfd6a0e -->
- [ ] **3. Test**: Verification → test-walkthrough.md <!-- tw:387579bf-bf0e-4455-b5f7-891a6a716594 -->

## Dependencies

### Requires
- None

### Enables
- None

## Success Criteria

- [ ] All 8 GET requests include `.error_for_status()?` <!-- tw:b5a12d89-a90b-4793-88a7-04ec36a32b56 -->
- [ ] Resume position errors logged to stderr <!-- tw:cc265481-edff-4d0e-b2cc-8b67971b5b4d -->
- [ ] `cargo check`, `cargo clippy`, and `cargo test` pass <!-- tw:864b92be-acfb-47a1-9373-b3505a0f6d1a -->
