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
- [ ] Queries unwatched items sorted by DateCreated descending <!-- tw:1338fccb-8398-4447-aaa1-b7e92112afd7 -->
- [ ] Finds items more recent than current item's DateCreated <!-- tw:9ef31bb7-5787-487c-8d53-c87cf855526d -->
- [ ] If more recent items exist, picks the most recent one <!-- tw:89d392e8-9675-4871-a32d-477d853ecab6 -->
- [ ] If no more recent items, picks the most recent less-recent one <!-- tw:921b8da6-541d-413c-834f-3f78efcaef38 -->
- [ ] Excludes current item from candidates <!-- tw:6b83ac1f-232f-4ace-894f-64c0dd1bbb44 -->
- [ ] Returns Ok(None) if no unwatched items at all <!-- tw:73915894-4f53-447b-ae02-360fccb48214 -->
- [ ] Handles items without DateCreated gracefully <!-- tw:6858a729-9a50-41d6-b219-fc27729909d7 -->
