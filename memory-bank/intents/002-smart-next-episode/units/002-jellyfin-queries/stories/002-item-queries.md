---
story: 002-item-queries
unit: 002-jellyfin-queries
intent: 002-smart-next-episode
priority: Must
estimate: M
---

## Story: Filtered Item Queries for Strategies

### Technical Story
**Description**: Add JellyfinClient methods to query items with filters needed by strategies.
**Rationale**: recent-unwatched needs DateCreated sort, random-unwatched needs random sort, series-or-random needs collection items.

### Acceptance Criteria
- [ ] `get_unwatched_items(library_id, sort_by, limit) -> Result<Vec<ItemSummary>>`
- [ ] Supports sort_by: DateCreated (desc), Random
- [ ] Filters: IsPlayed=false, ParentId=library_id
- [ ] `get_collection_items(collection_id) -> Result<Vec<ItemSummary>>` for box set contents
- [ ] `ItemSummary` struct with id, name, date_created, index_number, production_year
- [ ] Can exclude a specific item ID from results
- [ ] Deserialization tests with sample JSON
