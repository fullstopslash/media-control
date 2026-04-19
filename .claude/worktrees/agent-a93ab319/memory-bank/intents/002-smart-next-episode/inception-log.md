---
intent: 002-smart-next-episode
created: 2026-03-18T20:00:00Z
completed: 2026-03-18T20:30:00Z
status: complete
---

# Inception Log: smart-next-episode

## Overview

**Intent**: Per-library configurable "next episode" strategies for mark-watched-and-next
**Type**: new feature
**Created**: 2026-03-18

## Artifacts Created

| Artifact | Status | File |
|----------|--------|------|
| Requirements | ✅ | requirements.md |
| System Context | ✅ | system-context.md |
| Units | ✅ | units.md + 3 unit-brief.md |
| Stories | ✅ | 7 story files across 3 units |
| Bolt Plan | ✅ | 3 bolts (006-008) |

## Decision Log

| Date | Decision | Rationale | Approved |
|------|----------|-----------|----------|
| 2026-03-18 | Match rules by library name, not CollectionType | Pinchtube and Shows share CollectionType "tvshows" but need different strategies | Yes |
| 2026-03-18 | Use DateCreated for Pinchflat sort order | Reflects when content was acquired/downloaded | Yes |
| 2026-03-18 | Box sets for movie series detection | Jellyfin's native collection mechanism | Yes |
