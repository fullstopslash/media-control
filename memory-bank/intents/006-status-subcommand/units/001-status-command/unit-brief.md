---
unit: 001-status-command
intent: 006-status-subcommand
phase: inception
status: complete
created: 2026-03-19T19:00:00.000Z
updated: 2026-03-19T19:00:00.000Z
---

# Unit Brief: Status Command

## Purpose

Query mpv IPC socket for playback properties and output status in human-readable or JSON format. Designed for status bar integration (waybar) and scripting.

## Scope

### In Scope
- `query_mpv_property()` function returning serde_json::Value
- status.rs command with property querying + dual output format
- CLI wiring with --json flag

### Out of Scope
- Structured show/season/episode data (future: shim-side support needed)
- Hyprland or Jellyfin queries
- Daemon/polling mode

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Query mpv properties (4 get_property commands) | Must |
| FR-2 | Human-readable output | Must |
| FR-3 | JSON output (--json flag) | Must |
| FR-4 | query_mpv_property() function | Must |
| FR-5 | Not-playing detection (exit 1) | Must |
| FR-6 | CLI wiring | Must |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 3 |
| Must Have | 3 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-query-mpv-property | Query mpv property with response | Must | Planned |
| 002-status-command | Status command module | Must | Planned |
| 003-cli-wiring | Wire Status into main.rs | Must | Planned |

---

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| 004-ipc-reliability / 001-ipc-hardening | Socket discovery, validation |

### External Dependencies
| System | Purpose | Risk |
|--------|---------|------|
| mpv IPC socket | Property queries | Low |

---

## Success Criteria

### Functional
- [ ] `media-control status` shows human-readable playback state <!-- tw:ec57b58f-da23-4236-a185-e17045744e36 -->
- [ ] `media-control status --json` emits valid JSON <!-- tw:9cd8e18d-4efe-48de-8273-63fca2e77f3a -->
- [ ] Exit 0 when playing, exit 1 when not <!-- tw:3fbb142b-1ae7-4d9d-bcfe-8f582e93fc96 -->
- [ ] All 4 properties queried in single connection <!-- tw:a1ae2155-064a-4149-ac97-00f3a0720eb3 -->

### Non-Functional
- [ ] Response time < 50ms <!-- tw:68260a7c-4e04-4db0-bba1-3161314a0430 -->

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 011-status-command | simple-construction-bolt | all 3 | Full status subcommand |
