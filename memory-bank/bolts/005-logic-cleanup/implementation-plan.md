---
stage: plan
bolt: 005-logic-cleanup
created: 2026-03-18T19:00:00Z
---

## Implementation Plan: fullscreen/close cleanup + error consistency

### Objective
Simplify exit_fullscreen, deduplicate close's killwindow branches, and fix the semantically incorrect error in chapter.rs.

### Specific Changes

**Fullscreen (`fullscreen.rs`):**
1. Remove `_clients` parameter from `exit_fullscreen` - it's unused
2. Remove `#[allow(clippy::too_many_arguments)]` - fewer params now
3. Inline `exit_fullscreen_mode` into `exit_fullscreen` since the wrapper just unpacks MediaWindow fields
4. Simplify: `exit_fullscreen_mode` extracts 6 values from `media` and passes them individually to `exit_fullscreen` - just pass `media` and `clients` directly

**Close (`close.rs`):**
1. Merge the jellyfin and default killwindow branches - both do the same `dispatch("killwindow address:{addr}")` call
2. Structure: mpv early return → PiP error → killwindow fallthrough

**Chapter (`chapter.rs`):**
1. Replace `MediaControlError::WindowNotFound` with `MediaControlError::Io` wrapping a descriptive "no mpv IPC socket found" error
2. `WindowNotFound` is semantically wrong - we found the window, we just can't find its IPC socket

**Remaining `.map_err()` calls (mod.rs, mark_watched.rs):**
- These are legitimate conversions between error types without `From` impls (ConfigError→MediaControlError, JellyfinError→MediaControlError)
- No changes needed

### Acceptance Criteria
- [ ] `_clients` parameter removed from exit_fullscreen
- [ ] `#[allow(clippy::too_many_arguments)]` removed
- [ ] exit_fullscreen_mode simplified (fewer parameter unpacking)
- [ ] close has single killwindow path for non-mpv/non-PiP
- [ ] chapter.rs uses semantically correct error for missing socket
- [ ] All 173 tests pass
