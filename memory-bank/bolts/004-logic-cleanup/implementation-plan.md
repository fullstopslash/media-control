---
stage: plan
bolt: 004-logic-cleanup
created: 2026-03-18T18:00:00Z
---

## Implementation Plan: simplify avoid

### Objective
Restructure the avoid command for clarity: fix duplicate case numbering, reduce nesting, centralize the fullscreen guard.

### Current Issues

1. **Duplicate "Case 3" label** - lines 381 and 427 both say "Case 3" but are different logic:
   - First: multi-workspace + media focused (mouseover with geometry)
   - Second: multi-workspace + non-media focused (geometry overlap)

2. **Fullscreen guard repeated 3 times** - checked separately in Case 1 (line 304), Case 2 (line 333), and media-focused mouseover (line 383)

3. **Giant if/return chain** - the main function is ~200 lines of sequential if blocks with early returns

### Proposed Structure

Classify the scenario into an enum at the top, then match on it:

```
enum AvoidCase {
    SingleWorkspaceNonMedia,   // Case 1: move to primary position
    SingleWorkspaceMouseover,  // Case 2: toggle primary/secondary
    MultiWorkspaceMouseover,   // Case 3: geometry-based, restore focus
    MultiWorkspaceOverlap,     // Case 4: geometry-based overlap check
    FullscreenNonMedia,        // Case 5: non-media fullscreen, move away
}
```

Each case becomes a separate async function. The main `avoid()` function:
1. Check suppress → return early
2. Fetch clients
3. Find focused window → return early if none
4. Collect media windows → return early if none
5. Classify case
6. Dispatch to case handler

### Specific Changes

- **New**: `classify_case()` function that determines which case applies
- **New**: `handle_single_workspace_primary()` - Case 1
- **New**: `handle_mouseover_toggle()` - Case 2
- **New**: `handle_mouseover_geometry()` - Case 3 (was first "Case 3")
- **New**: `handle_overlap()` - Case 4 (was second "Case 3")
- **New**: `handle_fullscreen_nonmedia()` - Case 5 (was "Case 4")
- **Keep**: `calculate_target_position()`, `move_media_window()`, `get_position_pair()`, `should_suppress()`, `within_tolerance()`, `rectangles_overlap()` - these are clean already

### Acceptance Criteria
- [ ] All 15 avoid tests pass
- [ ] No function exceeds 4 nesting levels
- [ ] Each case is clearly named and separated
- [ ] No duplicate case labels
- [ ] Fullscreen guard handled once in classification
