---
story: 002-recent-unwatched
unit: 003-strategy-implementations
intent: 002-smart-next-episode
priority: Must
estimate: M
---

## Story: recent-unwatched Strategy

### Technical Story
**Description**: Implement the Pinchflat-style strategy that prefers more recently acquired unwatched content.
**Rationale**: YouTube-style libraries should play the newest unwatched content first, falling back to older if caught up.

### Acceptance Criteria
- [ ] Queries unwatched items sorted by DateCreated descending
- [ ] Finds items more recent than current item's DateCreated
- [ ] If more recent items exist, picks the most recent one
- [ ] If no more recent items, picks the most recent less-recent one
- [ ] Excludes current item from candidates
- [ ] Returns Ok(None) if no unwatched items at all
- [ ] Handles items without DateCreated gracefully
