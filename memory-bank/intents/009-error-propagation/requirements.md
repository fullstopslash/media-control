---
intent: 009-error-propagation
phase: inception
status: complete
created: 2026-03-19T00:00:00.000Z
updated: 2026-03-19T00:00:00.000Z
---

# Requirements: Error Propagation

## Intent Overview

Stop silently swallowing errors in command modules. Several functions use `.ok()` or `let _ =` to discard errors from Hyprland batch operations and mpv IPC calls. These silent failures make debugging difficult and hide real problems.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Surface Hyprland batch errors | `.ok()` replaced with `?` in move_media_window and fullscreen exit | Must |
| Surface mpv IPC errors in close | `let _ =` replaced with warning log | Must |
| Surface reposition errors in fullscreen exit | `.ok()` replaced with `?` on batch call | Must |

---

## Functional Requirements

### FR-1: Propagate Hyprland batch errors in move_media_window
- **Description**: In `avoid.rs`, `move_media_window` should propagate batch errors via `?` instead of swallowing with `.ok()`
- **Acceptance Criteria**: Batch failure returns `Err` to caller
- **Priority**: Must

### FR-2: Warn on mpv IPC errors in close
- **Description**: In `close.rs`, `send_mpv_script_message("stop-and-clear")` failures should be logged as warnings rather than silently discarded
- **Acceptance Criteria**: IPC failure prints to stderr but does not prevent further close logic
- **Priority**: Must

### FR-3: Propagate reposition errors in fullscreen exit
- **Description**: In `fullscreen.rs`, the batch call for repositioning after fullscreen exit should propagate errors via `?`
- **Acceptance Criteria**: Batch failure returns `Err` to caller
- **Priority**: Must

### FR-4: Handle non-critical suppression and avoid errors
- **Description**: `clear_suppression()` and `avoid()` calls after fullscreen exit should log failures at debug level rather than silently discarding
- **Acceptance Criteria**: Failures logged via `eprintln!` or `tracing::debug!`, do not prevent close completion
- **Priority**: Must

---

## Non-Functional Requirements

### Backward Compatibility
| Requirement | Metric | Target |
|-------------|--------|--------|
| No behavior change on success path | All existing tests pass | Must |

---

## Constraints

- All callers of modified functions already return `Result<()>` -- no signature changes needed
- Non-critical operations (suppression file, post-fullscreen avoid) should warn, not propagate

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| Callers handle `Result` properly | Error bubbles to CLI exit code | Already verified -- all callers use `?` |
