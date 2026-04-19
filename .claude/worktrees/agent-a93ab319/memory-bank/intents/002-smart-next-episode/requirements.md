---
intent: 002-smart-next-episode
phase: inception
status: complete
created: 2026-03-18T20:00:00Z
updated: 2026-03-18T20:00:00Z
---

# Requirements: Smart Next Episode

## Intent Overview

Per-library configurable "next episode" logic for mark-watched-and-next. Different Jellyfin libraries need different strategies for what to play after marking the current item as watched.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Seamless episode advancement across all library types | Pressing the key always plays something appropriate | Must |
| User-configurable per library | Config file controls behavior, no code changes needed | Must |
| Sensible defaults | Works without config for common cases | Should |

---

## Functional Requirements

### FR-1: Library Detection
- **Description**: Determine which Jellyfin library the currently playing item belongs to, using the item's ancestors API.
- **Acceptance Criteria**:
  - Given a NowPlayingItem, resolve its parent library name and ID via `/Items/{id}/Ancestors`
  - Cache the library info per session (don't re-fetch for every call)
  - Handle items with no library (graceful fallback)
- **Priority**: Must

### FR-2: Strategy Configuration
- **Description**: User-configurable next-episode strategies per library in `config.toml`.
- **Acceptance Criteria**:
  - Config supports rules matched by library name (string match)
  - Each rule specifies a strategy: `next-up`, `recent-unwatched`, `series-or-random`, `random-unwatched`
  - Rules are evaluated in order; first match wins
  - A rule with no `library` field acts as the default fallback
  - Missing config falls back to `next-up` (current behavior)
- **Priority**: Must
- **Config format**:
  ```toml
  [[next_episode.rules]]
  library = "Shows"
  strategy = "next-up"

  [[next_episode.rules]]
  library = "Pinchtube"
  strategy = "recent-unwatched"

  [[next_episode.rules]]
  library = "Movies"
  strategy = "series-or-random"

  [[next_episode.rules]]
  strategy = "random-unwatched"  # default
  ```

### FR-3: Strategy - next-up
- **Description**: Use Jellyfin's NextUp API to find the next unwatched episode in the series.
- **Acceptance Criteria**:
  - Calls `/Shows/{seriesId}/NextUp?UserId={userId}`
  - If NextUp returns an episode, play it
  - If no result (all watched or not a series), fall through to default
- **Priority**: Must

### FR-4: Strategy - recent-unwatched
- **Description**: Find the most recently acquired unwatched item in the same library.
- **Acceptance Criteria**:
  - Queries Jellyfin for unwatched items in the library, sorted by DateCreated descending
  - Picks items MORE recent than the current one first
  - If no more recent unwatched items exist, picks the most recent LESS recent one
  - Excludes the current item
  - Plays the selected item
- **Priority**: Must

### FR-5: Strategy - series-or-random
- **Description**: If the current item is part of a collection/box set, play the next in that collection. Otherwise pick a random unwatched item from the library.
- **Acceptance Criteria**:
  - Check if the item belongs to a Jellyfin collection (box set) via ancestors
  - If in a collection: get collection items sorted by production year/index, find next after current
  - If not in a collection: pick a random unwatched item from the library
  - Excludes the current item
- **Priority**: Must

### FR-6: Strategy - random-unwatched
- **Description**: Pick a random unwatched item from the same library.
- **Acceptance Criteria**:
  - Queries Jellyfin for unwatched items in the library
  - Picks one at random
  - Excludes the current item
- **Priority**: Must

### FR-7: Integration with mark-watched-and-next
- **Description**: Replace the current hardcoded NextUp fallback with the strategy system.
- **Acceptance Criteria**:
  - `mark_watched_and_next` resolves the library, looks up the strategy, executes it
  - Queue-based advancement still takes priority (if queue has remaining items, use those)
  - Strategy only runs when queue is empty
  - Errors in strategy execution don't prevent mark-watched from succeeding
- **Priority**: Must

---

## Non-Functional Requirements

### Performance
| Requirement | Metric | Target |
|-------------|--------|--------|
| Library resolution | API call latency | < 500ms (one HTTP call) |
| Strategy execution | Total latency | < 2s including Jellyfin API calls |

### Reliability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Fallback behavior | Strategy fails | Mark-watched still succeeds, no next played |
| Missing config | No rules defined | Falls back to next-up (current behavior) |

---

## Constraints

### Technical Constraints
- All Jellyfin API calls use the existing `JellyfinClient` and credential system
- No new external dependencies
- Config extends existing `config.toml` format (serde deserialization)
- Strategies are async (HTTP calls to Jellyfin)

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| Jellyfin Ancestors API reliably returns library info | Can't determine library | Fall back to default strategy |
| DateCreated reflects acquisition time for Pinchflat content | Wrong sort order | Could add alternative sort fields later |
| Box sets in Jellyfin contain ordered items | Movie series order wrong | Use ProductionYear as tiebreaker |
