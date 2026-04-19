---
id: 003-fullscreen-errors
unit: 001-error-propagation
intent: 009-error-propagation
status: complete
priority: must
created: 2026-03-19T00:00:00.000Z
assigned_bolt: 014-error-propagation
implemented: true
---

# Story: 003-fullscreen-errors

## User Story

**As a** media-control developer
**I want** reposition errors after fullscreen exit to propagate
**So that** failed repositioning is visible and debuggable

## Acceptance Criteria

- [x] **Given** the batch call for repositioning after fullscreen exit, **When** it fails, **Then** the error propagates via `?` <!-- tw:4442c19b-a780-4b03-8521-507713f7fa17 -->
- [x] **Given** `clear_suppression()` fails, **When** fullscreen exits, **Then** a warning is logged but exit continues <!-- tw:906ff00e-5650-474f-9850-feb6dd7f213f -->
- [x] **Given** `super::avoid::avoid(ctx)` fails after fullscreen exit, **When** called, **Then** a warning is logged but exit continues <!-- tw:974f7060-c084-4028-9ebe-7ff325cde3dc -->
- [x] **Given** existing fullscreen tests, **When** run after the change, **Then** all pass <!-- tw:33fe2476-67a9-4d40-82bf-b45578c560e9 -->

## Technical Notes

- Change `.ok()` to `?` on the batch call at ~line 216-217 of fullscreen.rs
- `clear_suppression()` failure is non-critical: use `.ok()` or `if let Err` with warning
- `avoid()` failure after fullscreen exit: use `if let Err(e)` with `eprintln!` or `tracing::debug!`
