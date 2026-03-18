---
story: 001-config-types
unit: 001-strategy-engine
intent: 002-smart-next-episode
priority: Must
estimate: S
---

## Story: Config Types and Parsing

### Technical Story
**Description**: Add `next_episode` config section with rules array to config.toml.
**Rationale**: Users need to configure different strategies per library.

### Acceptance Criteria
- [ ] `NextEpisodeRule` struct with optional `library` (String) and `strategy` (enum) <!-- tw:798749ae-8531-4e17-8a9c-d46099d094aa -->
- [ ] `NextEpisodeStrategy` enum: NextUp, RecentUnwatched, SeriesOrRandom, RandomUnwatched <!-- tw:5ea4403d-eca6-43e7-b823-1e2d073355f0 -->
- [ ] Config deserializes `[[next_episode.rules]]` TOML sections <!-- tw:7e365c80-fa0a-4acf-9123-d83cad90df9c -->
- [ ] Default config has no rules (falls back to next-up) <!-- tw:03e1c4bb-8ce3-4b79-87b3-e5e1196867f3 -->
- [ ] Rule with no `library` field matches any library (default/fallback) <!-- tw:552beaa9-e65f-4164-ab46-8dda94d33422 -->
- [ ] Unit test: parse sample config with multiple rules <!-- tw:40245903-ab5e-4e4a-9d7e-c4e1acf5a09f -->
