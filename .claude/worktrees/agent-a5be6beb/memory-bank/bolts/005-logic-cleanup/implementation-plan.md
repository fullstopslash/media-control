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
- [ ] `_clients` parameter removed from exit_fullscreen <!-- tw:20b2e0fd-6837-40fa-8d60-cea865622b0f -->
- [ ] `#[allow(clippy::too_many_arguments)]` removed <!-- tw:a64f5c1a-86b3-4352-a540-e367f84e4469 -->
- [ ] exit_fullscreen_mode simplified (fewer parameter unpacking) <!-- tw:2c332b0d-8775-4fe5-8c12-415627895bd0 -->
- [ ] close has single killwindow path for non-mpv/non-PiP <!-- tw:33bc82f6-ff7f-4525-93d2-001688e7e530 -->
- [ ] chapter.rs uses semantically correct error for missing socket <!-- tw:4bf752f0-0706-4586-b4c4-f47eb11b12c6 -->
- [ ] All 173 tests pass <!-- tw:5202ed92-4823-4836-a631-69bb404d5c8a -->
