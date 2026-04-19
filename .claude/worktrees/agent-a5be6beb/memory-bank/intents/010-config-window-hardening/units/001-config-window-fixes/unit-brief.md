---
unit: 001-config-window-fixes
intent: 010-config-window-hardening
phase: inception
status: complete
created: 2026-03-19T12:00:00.000Z
updated: 2026-03-19T12:00:00.000Z
---

# Unit Brief: Config & Window Fixes

## Purpose

Fix three independent but related edge cases: config loading resilience, hidden window filtering in find_media_window, and focus_history_id sort inversion in find_media_windows.

## Scope

### In Scope
- Config fallback to defaults on load failure
- Filter hidden/unmapped windows in find_media_window
- Fix focus_history_id -1 sort ordering

### Out of Scope
- Config schema changes
- New window matching features
- Changes to find_previous_focus (already correct)

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Config fallback on missing file | Must |
| FR-2 | Hidden/unmapped window filter | Must |
| FR-3 | focus_history_id sort fix | Must |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 3 |
| Must Have | 3 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-config-fallback | CLI falls back to defaults on missing config | Must | Planned |
| 002-hidden-window-filter | Filter hidden/unmapped windows in find_media_window | Must | Planned |
| 003-focus-history-sort | Fix focus_history_id -1 sort inversion | Must | Planned |

---

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| None | Standalone unit |

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 015-config-window-fixes | simple-construction-bolt | all 3 | Fix all three edge cases in one pass |
