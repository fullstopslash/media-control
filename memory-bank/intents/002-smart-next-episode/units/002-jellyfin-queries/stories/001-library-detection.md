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
- [ ] `get_item_library(item_id) -> Result<Option<LibraryInfo>>` method on JellyfinClient
- [ ] `LibraryInfo` struct with `id`, `name`, `collection_type` fields
- [ ] Calls `GET /Items/{id}/Ancestors` and finds the ancestor with `Type == "CollectionFolder"`
- [ ] Returns None if no library ancestor found
- [ ] Deserialization test with sample Ancestors JSON
