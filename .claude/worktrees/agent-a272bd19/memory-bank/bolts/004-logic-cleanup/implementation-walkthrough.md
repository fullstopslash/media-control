---
stage: implement
bolt: 004-logic-cleanup
created: 2026-03-18T18:30:00Z
---

## Implementation Walkthrough: simplify avoid

### Summary

Restructured the avoid command from a 200-line sequential if/return chain into an enum-based case classification with 5 separate handler functions. Each handler is under 30 lines.

### Completed Work

- [x] `crates/media-control-lib/src/commands/avoid.rs` - Rewrote `avoid()` function with `AvoidCase` enum and `classify_case()` dispatcher <!-- tw:b176214c-7283-4252-83ff-2a5e5da65215 -->

### Key Decisions

- **Enum-based dispatch**: `AvoidCase` enum with 5 variants makes the cases explicit and self-documenting
- **`classify_case()` function**: Centralizes all conditional logic (fullscreen guards, single/multi workspace, media/non-media focused) in one place
- **Separate handlers**: Each case is an independent async function with clear inputs
- **Single-workspace fullscreen**: Preserved original behavior where single-workspace + fullscreen non-media does nothing (don't interfere with fullscreen apps in simple layouts)

### Structure Change

**Before**: One 200-line function with nested if/return chains, duplicate "Case 3" labels, fullscreen guard in 3 places

**After**:
- `classify_case()` → determines which `AvoidCase` variant applies (or None)
- `avoid()` → suppress check, fetch clients, classify, match-dispatch
- `handle_move_to_primary()` → Case 1
- `handle_mouseover_toggle()` → Case 2
- `handle_mouseover_geometry()` → Case 3
- `handle_geometry_overlap()` → Case 4
- `handle_fullscreen_nonmedia()` → Case 5

### Deviations from Plan

- Fullscreen classification needed more nuance than planned: single-workspace fullscreen non-media returns None (no action), while multi-workspace fullscreen non-media returns FullscreenNonMedia. This preserves the original behavior where the old Case 1 short-circuited for fullscreen.
