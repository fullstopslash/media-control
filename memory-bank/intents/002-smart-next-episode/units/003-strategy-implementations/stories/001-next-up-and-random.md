---
story: 001-next-up-and-random
unit: 003-strategy-implementations
intent: 002-smart-next-episode
priority: Must
estimate: S
---

## Story: next-up and random-unwatched Strategies

### Technical Story
**Description**: Implement the two simplest strategies - they each make a single Jellyfin API call.
**Rationale**: next-up wraps the existing get_next_up, random-unwatched wraps get_unwatched_items with random sort.

### Acceptance Criteria
- [ ] next-up: calls get_next_up(series_id), plays result if found
- [ ] next-up: returns Ok(None) if no series_id or no next episode
- [ ] random-unwatched: calls get_unwatched_items(library_id, Random, limit=1)
- [ ] random-unwatched: excludes current item
- [ ] random-unwatched: returns Ok(None) if library has no unwatched items
- [ ] Both strategies return Option<String> (item ID to play, or None)
