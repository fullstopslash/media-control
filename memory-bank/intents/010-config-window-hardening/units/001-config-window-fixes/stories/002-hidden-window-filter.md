---
id: 002-hidden-window-filter
unit: 001-config-window-fixes
intent: 010-config-window-hardening
status: complete
priority: must
created: 2026-03-19T12:00:00.000Z
assigned_bolt: 015-config-window-fixes
implemented: true
---

# Story: 002-hidden-window-filter

## User Story

**As a** media-control user
**I want** hidden and unmapped windows excluded from media window search
**So that** commands don't target invisible windows that can't be interacted with

## Acceptance Criteria

- [ ] **Given** a hidden mpv window (hidden=true), **When** find_media_window runs, **Then** it is not returned <!-- tw:df24d077-1d03-428d-939d-c10f2c3a92e1 -->
- [-] **Given** an unmapped mpv window (mapped=false), **When** find_media_window runs, **Then** it is not returned <!-- tw:d5acf1ce-9d86-42fd-9d49-0dc7f5502627 -->
- [-] **Given** only hidden/unmapped media windows, **When** find_media_window runs, **Then** it returns None <!-- tw:74246833-904b-4ff6-acbb-7ee556cdd69c -->

## Technical Notes

- Add `.filter(|c| c.mapped && !c.hidden)` to the client iteration in `find_media_window`
- This matches the existing filter in `find_previous_focus`
- Also add the same filter to `find_media_windows` for consistency

## Dependencies

### Requires
- None

### Enables
- None
