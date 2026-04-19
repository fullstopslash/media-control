---
id: 001-avoid-errors
unit: 001-error-propagation
intent: 009-error-propagation
status: complete
priority: must
created: 2026-03-19T00:00:00.000Z
assigned_bolt: 014-error-propagation
implemented: true
---

# Story: 001-avoid-errors

## User Story

**As a** media-control developer
**I want** `move_media_window` to propagate Hyprland batch errors
**So that** failures in window repositioning are visible and debuggable

## Acceptance Criteria

- [x] **Given** `move_media_window` calls `ctx.hyprland.batch()`, **When** the batch fails, **Then** the error propagates to the caller via `?` <!-- tw:ade2bb5f-7073-4e15-9bc7-4e6ca9ec52b4 -->
- [x] **Given** all callers return `Result<()>`, **When** `?` is used, **Then** compilation succeeds <!-- tw:27066405-c486-4ea4-aadd-160183fb970f -->
- [x] **Given** existing tests, **When** run after the change, **Then** all pass <!-- tw:3ac05278-7e49-44ec-8520-2098219b8640 -->

## Technical Notes

- Change `.ok()` to `?` on the `ctx.hyprland.batch(&[...]).await` call in `move_media_window` (~line 170 of avoid.rs)
- All callers of `move_media_window` already propagate with `?`, so no signature changes needed
