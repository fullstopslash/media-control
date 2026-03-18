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
- [ ] `get_unwatched_items(library_id, sort_by, limit) -> Result<Vec<ItemSummary>>` <!-- tw:c92943c5-bf79-4f04-ba38-dbcc1692c5bc -->
- [ ] Supports sort_by: DateCreated (desc), Random <!-- tw:4f548248-d0f6-4d8a-8fa2-f269bcda2bfc -->
- [ ] Filters: IsPlayed=false, ParentId=library_id <!-- tw:b5b0a9e3-0330-44d0-84f5-581caa0014c2 -->
- [ ] `get_collection_items(collection_id) -> Result<Vec<ItemSummary>>` for box set contents <!-- tw:1f831cbf-56f8-451d-b2c9-1b613142103b -->
- [ ] `ItemSummary` struct with id, name, date_created, index_number, production_year <!-- tw:2f2a81ab-9a72-436f-a985-e86ddfb0c953 -->
- [ ] Can exclude a specific item ID from results <!-- tw:b2009332-eb68-4111-807d-d61b794c83b6 -->
- [ ] Deserialization tests with sample JSON <!-- tw:0a131584-8cff-4936-b195-ec17bee29ca3 -->
