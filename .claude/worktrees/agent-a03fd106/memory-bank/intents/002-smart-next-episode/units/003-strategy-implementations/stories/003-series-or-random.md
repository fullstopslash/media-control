---
story: 003-series-or-random
unit: 003-strategy-implementations
intent: 002-smart-next-episode
priority: Must
estimate: M
---

## Story: series-or-random Strategy

### Technical Story
**Description**: For movies - if part of a box set, play the next in the set. Otherwise random unwatched from library.
**Rationale**: Movie sequels should play in order, but standalone movies should just pick something new.

### Acceptance Criteria
- [ ] Check item ancestors for a BoxSet type <!-- tw:8728fb75-9be9-42d0-9be6-74123922fe0d -->
- [ ] If in box set: get collection items, find current item's position, play next <!-- tw:881c7ae2-1861-4a77-ba60-9d6a7cdd508a -->
- [ ] If next in box set doesn't exist (last in set), fall through to random <!-- tw:c3a7a46c-9533-41c8-9baf-7d7c71776081 -->
- [ ] If not in box set: delegate to random-unwatched strategy <!-- tw:baaf0554-f901-4653-a424-0d4eebc92f3e -->
- [ ] Returns Ok(None) if no candidates found <!-- tw:1a1f6683-87c3-48ca-aa4f-a5f76a977a1e -->
