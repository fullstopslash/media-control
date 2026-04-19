---
id: 001-mock-infrastructure
unit: 001-mock-infrastructure
intent: 001-test-and-refactor
type: simple-construction-bolt
status: complete
stories:
  - 001-mock-server
  - 002-command-capture
  - 003-test-context
created: 2026-03-18T13:00:00Z
started: 2026-03-18T14:00:00Z
completed: 2026-03-18T14:45:00Z
current_stage: null
stages_completed:
  - name: plan
    completed: 2026-03-18T14:15:00Z
    artifact: implementation-plan.md
  - name: implement
    completed: 2026-03-18T14:30:00Z
    artifact: implementation-walkthrough.md
  - name: test
    completed: 2026-03-18T14:45:00Z
    artifact: test-walkthrough.md

requires_bolts: []
enables_bolts: [002-test-coverage, 003-test-coverage]
requires_units: []
blocks: false

complexity:
  avg_complexity: 2
  avg_uncertainty: 1
  max_dependencies: 1
  testing_scope: 2
---

# Bolt: 001-mock-infrastructure

## Overview

Build the complete mock Hyprland IPC test infrastructure: mock socket server, command capture, and CommandContext test constructor.

## Objective

Provide the foundation that all E2E tests depend on. After this bolt, any command can be tested end-to-end without a running Hyprland instance.

## Stories Included

- **001-mock-server**: Mock Hyprland socket server (Must)
- **002-command-capture**: Command capture and assertion helpers (Must)
- **003-test-context**: CommandContext test constructor (Must)

## Bolt Type

**Type**: Simple Construction Bolt
**Definition**: `.specsmd/aidlc/templates/construction/bolt-types/simple-construction-bolt.md`

## Stages

- [ ] **1. Plan**: Design mock server API and test helper interfaces <!-- tw:8559f29d-8c72-4318-b852-254c86258bf9 -->
- [ ] **2. Implement**: Build mock server, capture, and test context <!-- tw:7f529fbe-c017-41b0-a127-0eb2eab6deb4 -->
- [ ] **3. Verify**: Test the mock infrastructure itself <!-- tw:5b4e27cf-1d85-46e0-a1cd-8d3b7194d293 -->

## Dependencies

### Requires
- None (foundation bolt)

### Enables
- 002-test-coverage (avoid + fullscreen tests)
- 003-test-coverage (simple command + edge case tests)

## Success Criteria

- [ ] Mock server responds to all Hyprland command types <!-- tw:2a58ca5b-0bea-489d-b979-b14e8969d521 -->
- [ ] Commands are captured and assertable <!-- tw:253a5017-b749-419a-b304-2a7f5b718a28 -->
- [ ] CommandContext can be built for any test scenario <!-- tw:7eac8030-2e17-4f2a-b2ee-e5d16595688d -->
- [ ] Mock infrastructure has its own tests <!-- tw:197d1042-5f67-427e-9aab-c4ad06cf4905 -->
