---
unit: 002-jellyfin-queries
intent: 002-smart-next-episode
phase: inception
status: ready
created: 2026-03-18T20:00:00Z
updated: 2026-03-18T20:00:00Z
unit_type: cli
default_bolt_type: simple-construction-bolt
---

# Unit Brief: Jellyfin Queries

## Purpose

Add JellyfinClient methods for library detection, filtered item queries, and collection item listing.

## Scope

### In Scope
- `get_item_library()` - resolve an item's parent library via Ancestors API
- `get_unwatched_items()` - query unwatched items in a library with sort options
- `get_collection_items()` - list items in a box set/collection
- Response types for these new endpoints

### Out of Scope
- Strategy logic (unit 003)
- Config parsing (unit 001)
- Existing methods (get_next_up, fetch_sessions, etc.) - already done

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Library Detection | Must |

## Story Summary

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-library-detection | Library detection via Ancestors | Must | Planned |
| 002-item-queries | Filtered item queries for strategies | Must | Planned |

## Dependencies

### Depends On
None

### Depended By
| Unit | Reason |
|------|--------|
| 003-strategy-implementations | Strategies call these query methods |
