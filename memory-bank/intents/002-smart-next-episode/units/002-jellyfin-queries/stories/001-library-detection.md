---
story: 001-library-detection
unit: 002-jellyfin-queries
intent: 002-smart-next-episode
priority: Must
estimate: S
---

## Story: Library Detection via Ancestors

### Technical Story
**Description**: Add a JellyfinClient method to determine which library an item belongs to.
**Rationale**: Strategy rules match by library name, so we need to know the library.

### Acceptance Criteria
- [ ] `get_item_library(item_id) -> Result<Option<LibraryInfo>>` method on JellyfinClient <!-- tw:31dbf4df-92c6-48c1-8d50-241aeee73721 -->
- [ ] `LibraryInfo` struct with `id`, `name`, `collection_type` fields <!-- tw:8a7eabf3-6c5a-41a5-98d2-4a277376293c -->
- [ ] Calls `GET /Items/{id}/Ancestors` and finds the ancestor with `Type == "CollectionFolder"` <!-- tw:f66fe1b8-2736-44f1-b23e-b5b97e54b9c2 -->
- [ ] Returns None if no library ancestor found <!-- tw:63dbe8b8-1bc3-488d-836d-9b856b01c379 -->
- [ ] Deserialization test with sample Ancestors JSON <!-- tw:a329bc5b-70bd-41e1-af41-aaaeddf71f6b -->
