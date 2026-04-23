---
id: 014-audit-round4-fixes
name: Audit Round 4 Fixes
status: planned
created: 2026-04-23T00:00:00Z
---

## Intent

Land all HIGH and MEDIUM severity findings from the round-4 multi-agent audit
of the media-control Rust workspace. Selected LOW findings are folded in where
they touch the same files.

## Scope

- HIGH: minify.rs and mark_watched.rs zero-test coverage
- MEDIUM: 14 findings spanning lib hardening, jellyfin client, command handlers, daemon, config
- LOW: dead code removal, error type consistency, case-insensitive parsers (folded into adjacent bolts)

Out of scope: jellyfin ID newtype refactor (deferred), commands/mod.rs split into submodules (deferred).

## Bolts

| ID | Files | Class | Parallel-safe |
|----|-------|-------|---------------|
| 019-audit-lib-hardening | error.rs, hyprland.rs, commands/mod.rs | MEDIUM/LOW | yes |
| 020-audit-jellyfin-hardening | jellyfin.rs | MEDIUM | yes |
| 021-audit-minify-fix | commands/minify.rs | HIGH+MEDIUM | yes |
| 022-audit-mark-watched-tests | commands/mark_watched.rs | HIGH | yes |
| 023-audit-handler-fixes | fullscreen, close, avoid, chapter, focus | MEDIUM/LOW | yes |
| 024-audit-daemon-config | daemon/main.rs, config.rs | MEDIUM | yes |

All six bolts touch disjoint files (or share only mod.rs with disjoint sections)
and can run concurrently in worktrees, then merge in any order.
