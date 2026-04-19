---
unit: 003-strategy-implementations
intent: 002-smart-next-episode
phase: inception
status: ready
created: 2026-03-18T20:00:00Z
updated: 2026-03-18T20:00:00Z
unit_type: cli
default_bolt_type: simple-construction-bolt
---

# Unit Brief: Strategy Implementations

## Purpose

Implement the 4 next-episode strategies using the engine from unit 001 and the Jellyfin queries from unit 002.

## Scope

### In Scope
- next-up strategy: calls get_next_up (existing)
- recent-unwatched strategy: queries by DateCreated, prefers newer
- series-or-random strategy: checks for box set membership, falls back to random
- random-unwatched strategy: queries with random sort

### Out of Scope
- Config parsing (unit 001)
- Jellyfin API methods (unit 002)
- Queue-based advancement (already works)

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-3 | next-up | Must |
| FR-4 | recent-unwatched | Must |
| FR-5 | series-or-random | Must |
| FR-6 | random-unwatched | Must |

## Story Summary

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-next-up-and-random | next-up and random-unwatched strategies | Must | Planned |
| 002-recent-unwatched | recent-unwatched strategy | Must | Planned |
| 003-series-or-random | series-or-random strategy | Must | Planned |

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| 001-strategy-engine | Strategy types and dispatch |
| 002-jellyfin-queries | API query methods |

### Depended By
None - this is the final unit.
