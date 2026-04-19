---
intent: 010-config-window-hardening
phase: inception
status: complete
created: 2026-03-19T12:00:00.000Z
updated: 2026-03-19T12:00:00.000Z
---

# Requirements: Config & Window Matching Hardening

## Intent Overview

Fix config loading and window matching edge cases. The CLI should not crash on missing config, `find_media_window` should skip hidden/unmapped windows (matching `find_previous_focus` behavior), and `find_media_windows` sort should not place never-focused windows (focus_history_id -1) before recently focused ones.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| CLI resilient to missing config | Falls back to defaults, logs debug message | Must |
| Hidden windows excluded from matching | find_media_window never returns hidden/unmapped | Must |
| Correct focus-based sort order | Never-focused windows sort last, not first | Must |

---

## Functional Requirements

### FR-1: Config Fallback on Missing File
- **Description**: When no `--config` flag is provided and the default config file is missing, fall back to `Config::default()` instead of returning an error.
- **Acceptance Criteria**: Running without a config file succeeds using built-in defaults; a debug-level log message is emitted.
- **Priority**: Must

### FR-2: Hidden/Unmapped Window Filter
- **Description**: `find_media_window` should filter out windows where `mapped == false` or `hidden == true`, consistent with `find_previous_focus`.
- **Acceptance Criteria**: Hidden or unmapped windows are never returned by `find_media_window`.
- **Priority**: Must

### FR-3: focus_history_id Sort Fix
- **Description**: In `find_media_windows`, windows with `focus_history_id == -1` (never focused) should sort after windows with non-negative IDs.
- **Acceptance Criteria**: A window with focus_history_id -1 appears after windows with focus_history_id 0, 1, 2, etc.
- **Priority**: Must

---

## Constraints

### Technical Constraints

- Rust workspace with tokio async runtime
- `Config::default()` already exists with sensible defaults
- `Client` struct already has `mapped` and `hidden` fields
