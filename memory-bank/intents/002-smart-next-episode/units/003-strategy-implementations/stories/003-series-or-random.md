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
- [ ] Check item ancestors for a BoxSet type
- [ ] If in box set: get collection items, find current item's position, play next
- [ ] If next in box set doesn't exist (last in set), fall through to random
- [ ] If not in box set: delegate to random-unwatched strategy
- [ ] Returns Ok(None) if no candidates found
