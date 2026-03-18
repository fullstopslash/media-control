---
stage: plan
bolt: 006-strategy-engine
created: 2026-03-18T20:30:00Z
---

## Implementation Plan: strategy engine

### Objective
Add config types for next-episode rules, strategy enum, rule matching, and wire it into the mark-watched-and-next flow.

### Deliverables
- New types in `config.rs`: `NextEpisodeConfig`, `NextEpisodeRule`, `NextEpisodeStrategy`
- New module `src/commands/next_strategy.rs`: strategy dispatch logic
- Modified `jellyfin.rs`: `mark_watched_and_next` takes a strategy parameter instead of hardcoding NextUp

### Technical Approach

**Config additions (config.rs):**
```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NextEpisodeConfig {
    pub rules: Vec<NextEpisodeRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NextEpisodeRule {
    pub library: Option<String>,  // None = default/fallback
    pub strategy: NextEpisodeStrategy,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NextEpisodeStrategy {
    NextUp,
    RecentUnwatched,
    SeriesOrRandom,
    RandomUnwatched,
}
```

Add `next_episode: NextEpisodeConfig` field to root `Config`.

**Strategy dispatch (next_strategy.rs):**
```rust
pub fn resolve_strategy(config: &Config, library_name: &str) -> NextEpisodeStrategy {
    for rule in &config.next_episode.rules {
        match &rule.library {
            Some(name) if name.eq_ignore_ascii_case(library_name) => return rule.strategy,
            None => return rule.strategy,  // default/fallback rule
            _ => continue,
        }
    }
    NextEpisodeStrategy::NextUp  // no rules at all → default
}
```

**Integration change (mark_watched.rs):**

The current `mark_watched_and_next` in `commands/mark_watched.rs` calls `jellyfin.mark_watched_and_next()` which hardcodes the NextUp fallback. Instead:

1. `mark_watched_and_next` marks watched via jellyfin
2. Tries queue advancement (existing)
3. If queue empty: resolves library → matches strategy → calls strategy executor
4. Strategy executor is a stub in this bolt (returns Ok(None)), implemented in bolt 008

This means we need to split `jellyfin.mark_watched_and_next()` into separate concerns:
- `jellyfin.mark_watched(item_id)` - already exists
- `jellyfin.get_remaining_queue_ids()` - already exists (make pub)
- `jellyfin.play_item(session_id, item_id)` - already exists
- Strategy resolution moves to commands/mark_watched.rs

### Acceptance Criteria
- [ ] Config parses `[[next_episode.rules]]` with strategy enum
- [ ] Default config has no rules (empty vec)
- [ ] `resolve_strategy()` matches first matching rule, falls back to NextUp
- [ ] Case-insensitive library name matching
- [ ] mark_watched_and_next uses strategy dispatch instead of hardcoded NextUp
- [ ] All existing tests pass
