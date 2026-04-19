---
unit: 001-error-propagation
intent: 009-error-propagation
phase: inception
status: complete
created: 2026-03-19T00:00:00.000Z
updated: 2026-03-19T00:00:00.000Z
---

# Unit Brief: Error Propagation

## Purpose

Replace silent error swallowing (`.ok()`, `let _ =`) with proper error propagation or warning logs across command modules. Ensures failures are visible for debugging.

## Scope

### In Scope
- `avoid.rs` `move_media_window`: `.ok()` to `?`
- `close.rs`: `let _ =` on mpv IPC to `if let Err` warning
- `fullscreen.rs`: `.ok()` on reposition batch to `?`; `clear_suppression`/`avoid` to logged errors

### Out of Scope
- New error types or error recovery strategies
- Changes to public API signatures

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Propagate batch errors in move_media_window | Must |
| FR-2 | Warn on mpv IPC errors in close | Must |
| FR-3 | Propagate reposition errors in fullscreen exit | Must |
| FR-4 | Handle non-critical suppression/avoid errors | Must |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 3 |
| Must Have | 3 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-avoid-errors | Propagate Hyprland batch errors in move_media_window | Must | Complete |
| 002-close-errors | Propagate mpv IPC errors in close | Must | Complete |
| 003-fullscreen-errors | Propagate reposition errors in fullscreen exit | Must | Complete |

---

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| None | Modifies existing code only |

### External Dependencies
| System | Purpose | Risk |
|--------|---------|------|
| None | | |

---

## Success Criteria

### Functional
- [x] `move_media_window` propagates batch errors <!-- tw:1f04b02b-d5f0-4680-bf03-77de755ff68e -->
- [x] `close` warns on mpv IPC failure instead of silently discarding <!-- tw:2b7ae11c-7ed8-4c99-91e6-1861154b401a -->
- [x] Fullscreen exit propagates reposition batch errors <!-- tw:2806be4a-2ab3-4d9c-921c-67b701408050 -->
- [x] Non-critical errors (suppression, post-fullscreen avoid) are logged <!-- tw:3a62fe52-ce21-4949-9196-d192ca8cef80 -->

### Non-Functional
- [x] All existing tests pass without modification <!-- tw:70181135-79b5-4416-89fa-656164de0a36 -->

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 014-error-propagation | simple-construction-bolt | all 3 | Error propagation pass |
