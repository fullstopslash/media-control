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
- [ ] next-up: calls get_next_up(series_id), plays result if found <!-- tw:cd6a1e69-f602-4869-b46a-91a59ef03a9f -->
- [ ] next-up: returns Ok(None) if no series_id or no next episode <!-- tw:be05e9da-b0e1-45e7-9892-3af3114806c1 -->
- [ ] random-unwatched: calls get_unwatched_items(library_id, Random, limit=1) <!-- tw:ba7f588c-91b2-4687-b8e5-69c5bf78ec3a -->
- [ ] random-unwatched: excludes current item <!-- tw:2b01e233-55f7-4f3e-9b3e-f26f8caade18 -->
- [ ] random-unwatched: returns Ok(None) if library has no unwatched items <!-- tw:b871056f-9a2f-42ae-a4d5-0e8fc24e13a9 -->
- [ ] Both strategies return Option<String> (item ID to play, or None) <!-- tw:7c2509d0-2c49-49a0-bcf6-7d304715fabb -->
