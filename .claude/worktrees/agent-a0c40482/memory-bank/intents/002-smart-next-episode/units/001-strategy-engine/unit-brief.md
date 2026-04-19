---
unit: 001-strategy-engine
intent: 002-smart-next-episode
phase: inception
status: ready
created: 2026-03-18T20:00:00Z
updated: 2026-03-18T20:00:00Z
unit_type: cli
default_bolt_type: simple-construction-bolt
---

# Unit Brief: Strategy Engine

## Purpose

Define the strategy abstraction, config parsing for next-episode rules, and integration point in mark-watched-and-next.

## Scope

### In Scope
- `NextEpisodeStrategy` enum (next-up, recent-unwatched, series-or-random, random-unwatched)
- `NextEpisodeRule` config struct (library name + strategy)
- Config parsing: `[[next_episode.rules]]` section in config.toml
- Rule matching: given a library name, find the first matching rule
- Integration: replace hardcoded NextUp in mark-watched-and-next with strategy dispatch

### Out of Scope
- Actual Jellyfin API calls (unit 002)
- Strategy implementations (unit 003)

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-2 | Strategy Configuration | Must |
| FR-7 | Integration with mark-watched-and-next | Must |

## Story Summary

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-config-types | Config types and parsing | Must | Planned |
| 002-strategy-dispatch | Strategy dispatch and integration | Must | Planned |

## Dependencies

### Depends On
None

### Depended By
| Unit | Reason |
|------|--------|
| 003-strategy-implementations | Needs strategy trait and config types |
