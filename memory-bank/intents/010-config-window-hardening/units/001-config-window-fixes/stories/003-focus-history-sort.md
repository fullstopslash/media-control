---
id: 003-focus-history-sort
unit: 001-config-window-fixes
intent: 010-config-window-hardening
status: complete
priority: must
created: 2026-03-19T12:00:00.000Z
assigned_bolt: 015-config-window-fixes
implemented: true
---

# Story: 003-focus-history-sort

## User Story

**As a** media-control user
**I want** never-focused windows to sort last in media window lists
**So that** recently focused windows are preferred over windows that were never interacted with

## Acceptance Criteria

- [ ] **Given** windows with focus_history_id [0, 2, -1], **When** find_media_windows sorts, **Then** order is [0, 2, -1] <!-- tw:ddcf42e7-d8a4-4bdb-a196-169e575101bc -->
- [ ] **Given** all windows have focus_history_id -1, **When** find_media_windows sorts, **Then** they remain in stable order <!-- tw:0fb2704a-1ee7-4c64-b2ef-ca498e215045 -->
- [ ] **Given** windows with focus_history_id [1, -1, 0], **When** find_media_windows sorts (same priority), **Then** order is [0, 1, -1] <!-- tw:32ff9950-ecfe-43e5-8c6c-0ced5ae3690d -->

## Technical Notes

- focus_history_id -1 means "never focused" in Hyprland
- Current sort: `a.focus_history_id.cmp(&b.focus_history_id)` puts -1 before 0
- Fix: special-case -1 to sort Greater (last)
- Only affects `find_media_windows`, not `find_media_window` (single result)

## Dependencies

### Requires
- None

### Enables
- None
