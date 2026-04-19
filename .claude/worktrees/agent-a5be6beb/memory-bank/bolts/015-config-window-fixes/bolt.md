---
id: 015-config-window-fixes
unit: 001-config-window-fixes
intent: 010-config-window-hardening
type: simple-construction-bolt
status: complete
stories:
  - 001-config-fallback
  - 002-hidden-window-filter
  - 003-focus-history-sort
created: 2026-03-19T12:00:00.000Z
started: 2026-03-19T12:00:00.000Z
completed: "2026-03-20T04:34:35Z"
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

# Bolt: 015-config-window-fixes

## Overview

Fix three config and window matching edge cases: config fallback to defaults, hidden/unmapped window filtering, and focus_history_id sort inversion.

## Objective

Eliminate edge cases where the CLI crashes on missing config, targets invisible windows, or prefers never-focused windows over recently focused ones.

## Stories Included

- **001-config-fallback**: CLI falls back to Config::default() on missing config (Must)
- **002-hidden-window-filter**: Filter hidden/unmapped windows in find_media_window (Must)
- **003-focus-history-sort**: Fix focus_history_id -1 sort inversion (Must)

## Implementation Plan

### Story 001 — Config Fallback
- File: `crates/media-control/src/main.rs`
- Change: `Config::load()?` to `Config::load().unwrap_or_else(|e| { tracing::debug!(...); Config::default() })`

### Story 002 — Hidden Window Filter
- File: `crates/media-control-lib/src/window.rs`
- Change: Add `.filter(|c| c.mapped && !c.hidden)` to find_media_window and find_media_windows client iteration

### Story 003 — Focus History Sort
- File: `crates/media-control-lib/src/window.rs`
- Change: Special-case focus_history_id < 0 to sort last in find_media_windows
