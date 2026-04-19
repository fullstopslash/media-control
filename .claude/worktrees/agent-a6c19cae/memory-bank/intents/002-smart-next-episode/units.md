---
intent: 002-smart-next-episode
phase: inception
status: units-decomposed
updated: 2026-03-18T20:00:00Z
---

# Smart Next Episode - Unit Decomposition

## Units Overview

This intent decomposes into 3 units:

### Unit 1: 001-strategy-engine

**Description**: The strategy trait, enum-based dispatch, and config parsing for next-episode rules.

**Assigned Requirements**: FR-2 (config), FR-7 (integration)
**Deliverables**: Strategy trait, config types, rule matching, integration with mark-watched-and-next
**Dependencies**: None
**Estimated Complexity**: M

### Unit 2: 002-jellyfin-queries

**Description**: Jellyfin API queries needed by strategies: library detection, filtered item queries, collection queries.

**Assigned Requirements**: FR-1 (library detection)
**Deliverables**: New JellyfinClient methods for ancestors, filtered items, collections
**Dependencies**: None (parallel with unit 1)
**Estimated Complexity**: M

### Unit 3: 003-strategy-implementations

**Description**: Implement the 4 strategy types using the engine and Jellyfin queries.

**Assigned Requirements**: FR-3 (next-up), FR-4 (recent-unwatched), FR-5 (series-or-random), FR-6 (random-unwatched)
**Deliverables**: All 4 strategy implementations, wired into the engine
**Dependencies**: 001-strategy-engine, 002-jellyfin-queries
**Estimated Complexity**: L

## Requirement-to-Unit Mapping

- **FR-1**: Library Detection → `002-jellyfin-queries`
- **FR-2**: Strategy Configuration → `001-strategy-engine`
- **FR-3**: next-up → `003-strategy-implementations`
- **FR-4**: recent-unwatched → `003-strategy-implementations`
- **FR-5**: series-or-random → `003-strategy-implementations`
- **FR-6**: random-unwatched → `003-strategy-implementations`
- **FR-7**: Integration → `001-strategy-engine`

## Unit Dependency Graph

```text
[001-strategy-engine] ──► [003-strategy-implementations]
[002-jellyfin-queries] ──► [003-strategy-implementations]
```

## Execution Order

1. Units 001 + 002 (parallel - no dependencies on each other)
2. Unit 003 (needs both)
