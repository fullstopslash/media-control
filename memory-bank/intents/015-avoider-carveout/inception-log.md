---
intent: 015-avoider-carveout
created: 2026-04-26T00:00:00Z
completed: 2026-04-26T00:00:00Z
status: complete
---

# Inception Log: 015-avoider-carveout

## Overview

**Intent**: Carve the window-avoidance daemon cleanly away from the Jellyfin/mpv-shim workflow code inside the existing media-control workspace. Apply a focused efficiency/clarity/DRY pass to `avoid.rs` and the daemon hot path.

**Type**: refactoring

**Created**: 2026-04-26

## Artifacts Created

| Artifact | Status | File |
|----------|--------|------|
| Requirements | ✅ | requirements.md |
| System Context | ✅ | system-context.md |
| Units | ✅ | units.md |
| Unit Brief: 001-commands-regrouping | ✅ | units/001-commands-regrouping/unit-brief.md |
| Unit Brief: 002-daemon-substrate-tightening | ✅ | units/002-daemon-substrate-tightening/unit-brief.md |
| Unit Brief: 003-avoider-cleanup | ✅ | units/003-avoider-cleanup/unit-brief.md |
| Stories: 001-commands-regrouping (3) | ✅ | units/001-commands-regrouping/stories/*.md |
| Stories: 002-daemon-substrate-tightening (2) | ✅ | units/002-daemon-substrate-tightening/stories/*.md |
| Stories: 003-avoider-cleanup (6) | ✅ | units/003-avoider-cleanup/stories/*.md |
| Bolt Plan | ✅ | memory-bank/bolts/025-commands-regrouping/bolt.md, 026-daemon-substrate-tightening/bolt.md, 027-avoider-cleanup/bolt.md |

## Summary

| Metric | Count |
|--------|-------|
| Functional Requirements | 5 |
| Non-Functional Requirements | 4 (perf x2, reliability x2, maintainability x2) |
| Units | 3 |
| Stories | 11 |
| Bolts Planned | 3 |

## Units Breakdown

| Unit | Stories | Bolts | Priority |
|------|---------|-------|----------|
| 001-commands-regrouping | 3 | 1 (025) | Must |
| 002-daemon-substrate-tightening | 2 | 1 (026) | Must |
| 003-avoider-cleanup | 6 | 1 (027; may split to 027a/027b at construction) | Must |

## Decision Log

| Date | Decision | Rationale | Approved |
|------|----------|-----------|----------|
| 2026-04-26 | No repo split | User explicitly excluded; binaries share substrate (hyprland, window, config, error, test_helpers) at runtime; composite CLI commands cross both worlds | Yes (intent owner) |
| 2026-04-26 | No new top-level crates | Module reorganization inside `media-control-lib` is sufficient for the carve-out; lib-split is a future intent if ever needed | Yes |
| 2026-04-26 | 3-unit decomposition (regroup → tighten → cleanup) | Strict linear chain; each unit's preconditions are the prior unit's deliverables; ships small, reviewable PRs | Yes |
| 2026-04-26 | One bolt per unit (025, 026, 027) | Matches existing project convention (intent 014 etc.); 027 may split at construction if it grows unwieldy | Yes |
| 2026-04-26 | Cleanup hit list elevated to first-class FR (FR-3) | User specifically asked for the avoider to be "absolutely efficient, clean, and DRY"; not an afterthought | Yes |
| 2026-04-26 | Daemon-owned hot-path state (FR-4) sequenced after substrate tightening | The carve-out is what *enables* daemon-only state; doing it first is what justifies separating the daemon | Yes |
| 2026-04-26 | Test infrastructure stays single-source (FR-5) | `test_helpers.rs` is the one mock layer; daemon must not grow its own | Yes |
| 2026-04-26 | Enforcement mechanism for FR-2 deferred to bolt 026 design stage | Intent owner uncertain between three viable options (cargo feature, `pub(crate)` + facade, `compile_fail` doctest); choice depends on existing project conventions discoverable at construction time | Deferred (open question) |
| 2026-04-26 | Cache invalidation strategy (FR-4 / story 005): **TTL-of-one-debounce** | Intent owner picked TTL: simpler than event-driven; no Hyprland event-taxonomy assumptions; covers the burst-fired-event common case; conservative correctness over peak efficiency. Story 005 and bolt 027 updated. | Yes |

## Scope Changes

| Date | Change | Reason | Impact |
|------|--------|--------|--------|
| (none) | — | — | — |

## Ready for Construction

**Checklist**:

- [x] All requirements documented
- [x] System context defined
- [x] Units decomposed
- [x] Stories created for all units (11 stories)
- [x] Bolts planned (3 bolts)
- [ ] Human review complete

## Next Steps

1. Begin Construction Phase
2. Start with Unit: `001-commands-regrouping` (bolt 025)
3. Execute: `/specsmd-construction-agent --intent="015-avoider-carveout"`

## Dependencies

Linear unit chain:

```text
[001-commands-regrouping] → [002-daemon-substrate-tightening] → [003-avoider-cleanup]
```

Bolt sequence:

```text
025-commands-regrouping → 026-daemon-substrate-tightening → 027-avoider-cleanup
```

## Inputs Used

- **Coupling survey** (Explore subagent, 2026-04-26): Confirmed daemon imports only `CommandContext`, `runtime_dir`, `Config`, `MediaControlError`, `HyprlandError`, `runtime_socket_path`; `avoid.rs` has zero workflow/Jellyfin imports; window-mgmt vs. workflow split is a clean cut.
- **Cleanup audit** (unix-code-critic subagent, 2026-04-26): Produced a prioritized 15-item hit list with file:line citations and a "what NOT to touch" backstop. Drives FR-3 sub-items and the bolt 027 stories.
