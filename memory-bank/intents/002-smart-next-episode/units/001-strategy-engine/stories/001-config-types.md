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
- [ ] `NextEpisodeRule` struct with optional `library` (String) and `strategy` (enum)
- [ ] `NextEpisodeStrategy` enum: NextUp, RecentUnwatched, SeriesOrRandom, RandomUnwatched
- [ ] Config deserializes `[[next_episode.rules]]` TOML sections
- [ ] Default config has no rules (falls back to next-up)
- [ ] Rule with no `library` field matches any library (default/fallback)
- [ ] Unit test: parse sample config with multiple rules
