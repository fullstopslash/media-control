---
stage: implement
bolt: 006-strategy-engine
created: 2026-03-18T21:00:00Z
---

## Implementation Walkthrough: strategy engine

### Summary

Added per-library next-episode strategy configuration, library detection via Jellyfin Ancestors API, and restructured mark-watched-and-next to dispatch strategies instead of hardcoding NextUp.

### Completed Work

- [x] `crates/media-control-lib/src/config.rs` - Added `NextEpisodeConfig`, `NextEpisodeRule`, `NextEpisodeStrategy` types with TOML parsing <!-- tw:bfc43ae2-3bd6-46fe-8a13-7ed98f52abb6 -->
- [x] `crates/media-control-lib/src/config.rs` - Added `resolve_strategy()` with first-match semantics and case-insensitive library names <!-- tw:9045d851-1254-478e-be42-5bc13829956b -->
- [x] `crates/media-control-lib/src/commands/mark_watched.rs` - Restructured `mark_watched_and_next` to own strategy dispatch <!-- tw:fb5c0005-0bc3-447f-850f-aa23176f3b17 -->
- [x] `crates/media-control-lib/src/commands/mark_watched.rs` - Added `execute_next_strategy()` with NextUp implemented, other strategies stubbed <!-- tw:11158fdf-078c-4ca0-9ffb-40abb862a0ab -->
- [x] `crates/media-control-lib/src/jellyfin.rs` - Added `get_item_library()` via Ancestors API, `LibraryInfo` and `AncestorItem` types <!-- tw:61d6380a-f58e-430e-b0d5-118a8cea0893 -->
- [x] `crates/media-control-lib/src/jellyfin.rs` - Made `get_remaining_queue_ids` public <!-- tw:a778b0f2-536f-4f24-97f4-784a93faf843 -->

### Key Decisions

- **Strategy dispatch in commands, not jellyfin.rs**: The command layer has access to config; the Jellyfin client stays a pure API client
- **Stubbed strategies fall back to NextUp**: Until bolt 008 implements them, all strategies use NextUp as a sensible default
- **Library detection via Ancestors**: One API call to resolve the library, searching for `CollectionFolder` type ancestor
- **Strategy errors are best-effort**: mark-watched always succeeds; strategy failures just mean no next episode plays
