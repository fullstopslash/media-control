---
id: 002-close-errors
unit: 001-error-propagation
intent: 009-error-propagation
status: complete
priority: must
created: 2026-03-19T00:00:00.000Z
assigned_bolt: 014-error-propagation
implemented: true
---

# Story: 002-close-errors

## User Story

**As a** media-control developer
**I want** mpv IPC errors in close to be visible
**So that** failed stop-and-clear commands are debuggable

## Acceptance Criteria

- [x] **Given** `send_mpv_script_message("stop-and-clear")` fails, **When** close is called for an mpv window, **Then** the error is propagated or logged as a warning <!-- tw:7049e5a1-6e2e-41c8-bd43-7513461c4cfd -->
- [x] **Given** mpv IPC fails, **When** the window is not mpv, **Then** close still attempts closewindow dispatch <!-- tw:016c2152-7bd6-46a3-b3fe-a8f64d80501b -->
- [x] **Given** existing close tests, **When** run after the change, **Then** all pass <!-- tw:33391fcc-d580-4ecf-b4b4-655e885b828d -->

## Technical Notes

- In `close.rs`, the mpv branch returns early after `send_mpv_script_message`, so propagation via `?` is acceptable (there is no subsequent closewindow to attempt)
- For non-mpv paths, errors already propagate correctly
