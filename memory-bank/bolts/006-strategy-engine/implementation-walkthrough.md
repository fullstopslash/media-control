---
stage: implement
bolt: 006-strategy-engine
created: 2026-03-18T21:00:00Z
---

## Implementation Walkthrough: strategy engine

### Summary

Added per-library next-episode strategy configuration, library detection via Jellyfin Ancestors API, and restructured mark-watched-and-next to dispatch strategies instead of hardcoding NextUp.

### Completed Work

- [x] `crates/media-control-lib/src/config.rs` - Added `NextEpisodeConfig`, `NextEpisodeRule`, `NextEpisodeStrategy` types with TOML parsing
- [x] `crates/media-control-lib/src/config.rs` - Added `resolve_strategy()` with first-match semantics and case-insensitive library names
- [x] `crates/media-control-lib/src/commands/mark_watched.rs` - Restructured `mark_watched_and_next` to own strategy dispatch
- [x] `crates/media-control-lib/src/commands/mark_watched.rs` - Added `execute_next_strategy()` with NextUp implemented, other strategies stubbed
- [x] `crates/media-control-lib/src/jellyfin.rs` - Added `get_item_library()` via Ancestors API, `LibraryInfo` and `AncestorItem` types
- [x] `crates/media-control-lib/src/jellyfin.rs` - Made `get_remaining_queue_ids` public

### Key Decisions

- **Strategy dispatch in commands, not jellyfin.rs**: The command layer has access to config; the Jellyfin client stays a pure API client
- **Stubbed strategies fall back to NextUp**: Until bolt 008 implements them, all strategies use NextUp as a sensible default
- **Library detection via Ancestors**: One API call to resolve the library, searching for `CollectionFolder` type ancestor
- **Strategy errors are best-effort**: mark-watched always succeeds; strategy failures just mean no next episode plays
