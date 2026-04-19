---
stage: implement
bolt: 005-logic-cleanup
created: 2026-03-18T19:30:00Z
---

## Implementation Walkthrough: fullscreen/close cleanup + error consistency

### Summary

Simplified fullscreen exit (merged wrapper, removed unused param), deduplicated close killwindow branches, and fixed semantically incorrect error in chapter.rs.

### Completed Work

- [x] `crates/media-control-lib/src/commands/fullscreen.rs` - Merged `exit_fullscreen_mode` into `exit_fullscreen`, removed `_clients` param, removed `#[allow(clippy::too_many_arguments)]` <!-- tw:07befbd5-8ba0-49c4-a1f6-16e1dd4d73a3 -->
- [x] `crates/media-control-lib/src/commands/close.rs` - Merged jellyfin and default killwindow into single fallthrough <!-- tw:32cfe771-4e97-4f18-8135-63f376528712 -->
- [x] `crates/media-control-lib/src/commands/chapter.rs` - Replaced `WindowNotFound` with `Io(NotFound)` for missing mpv socket <!-- tw:391035b3-57b2-4d37-8580-904e21dbca5d -->

### Key Decisions

- **`should_restore_pin` absorbs `was_pinned`**: The old code checked `should_restore_pin || was_pinned`, but `should_restore_pin` already includes `media.pinned` in its calculation, so `was_pinned` was redundant
- **Close deduplication**: The jellyfin-specific killwindow branch was identical to the default. Only mpv (playerctl) and PiP (error) need special handling; everything else is killwindow

### Deviations from Plan

None
