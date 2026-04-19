---
id: 014-error-propagation
unit: 001-error-propagation
intent: 009-error-propagation
type: simple-construction-bolt
status: complete
stories:
  - 001-avoid-errors
  - 002-close-errors
  - 003-fullscreen-errors
created: 2026-03-19T00:00:00.000Z
started: 2026-03-19T00:00:00.000Z
completed: 2026-03-19T00:00:00.000Z
current_stage: null
stages_completed:
  - name: implement
    completed: 2026-03-19T00:00:00.000Z
    artifact: null
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

# Bolt: 014-error-propagation

## Overview

Replace silent error swallowing with proper error propagation and warning logs across avoid, close, and fullscreen command modules.

## Objective

Make errors visible for debugging instead of silently discarding them.

## Stories Included

- **001-avoid-errors**: Propagate Hyprland batch errors in move_media_window (Must)
- **002-close-errors**: Propagate mpv IPC errors in close (Must)
- **003-fullscreen-errors**: Propagate reposition errors in fullscreen exit (Must)

## Bolt Type

**Type**: Simple Construction Bolt

## Changes Made

### avoid.rs - move_media_window
- Changed `.ok()` to `?` on `ctx.hyprland.batch()` call so batch errors propagate to callers

### close.rs - close_window_gracefully
- `send_mpv_script_message("stop-and-clear")` now propagates via `?` (mpv branch returns early, no subsequent closewindow to protect)

### fullscreen.rs - exit_fullscreen
- Changed `.ok()` to `?` on the reposition batch call after fullscreen exit
- `clear_suppression()`: uses `if let Err(e)` with `eprintln!` warning (non-critical)
- `super::avoid::avoid(ctx)`: uses `if let Err(e)` with `eprintln!` warning (non-critical)

## Dependencies

### Requires
- None

### Enables
- None

## Success Criteria

- [x] All 3 stories implemented <!-- tw:2e229be4-385d-48d1-9057-9347459880b6 -->
- [x] `cargo check` passes <!-- tw:8a66588e-24c6-4cc6-9f32-49d7d57cd419 -->
- [x] `cargo clippy` passes <!-- tw:9b15f0e3-3feb-4d89-b5d9-0051af2d0eeb -->
- [x] `cargo test` passes <!-- tw:8a26af0f-5d68-4f50-bc95-e73749c754b7 -->
- [x] No behavior change on success path <!-- tw:8ebece38-f956-4dab-bcee-ad7a4eee1166 -->
