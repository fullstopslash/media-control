---
unit: 002-rollout-migration
intent: 018-sock-trigger-ipc
created: 2026-05-03T16:45:54Z
last_updated: 2026-05-03T16:45:54Z
---

# Construction Log: rollout-migration

## Original Plan

**From Inception**: 1 bolt planned (031)
**Planned Date**: 2026-05-03

| Bolt ID | Stories | Type |
|---------|---------|------|
| 031-rollout-migration | 001-hyprland-keybind-migration, 002-nixos-module-cleanup, 003-end-to-end-validation | simple-construction-bolt |

## Replanning History

| Date | Action | Change | Reason | Approved |
|------|--------|--------|--------|----------|

## Current Bolt Structure

Single bolt covering all 3 stories of unit 002. Cross-repo activation: keybind file edit + nix module edit + DoD validation. Sequential within bolt.

## Construction Log

- **2026-05-03T16:45:54Z**: 031-rollout-migration started — Stage 1: plan
- **2026-05-03T16:45:54Z**: Pre-flight check — installed `media-control` (`/run/current-system/sw/bin/media-control`) does NOT yet have `kick` subcommand. Bolt 030's release has not been shipped through nixos-rebuild. Stage 1 plan must address rollout sequencing before Stage 2 can land safely.
