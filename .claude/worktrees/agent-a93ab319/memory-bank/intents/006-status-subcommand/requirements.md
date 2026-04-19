---
intent: 006-status-subcommand
phase: inception
status: complete
created: 2026-03-19T19:00:00.000Z
updated: 2026-03-19T19:00:00.000Z
---

# Requirements: Status Subcommand

## Intent Overview

Add `media-control status` command that queries mpv's IPC socket for current playback state. Default output is human-readable; `--json` emits machine-parseable JSON for status bar integration (waybar/polybar).

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Expose playback state from CLI | Status output matches mpv state | Must |
| Waybar/polybar integration | `--json` flag emits valid JSON | Must |
| Fast response for status bar polling | < 50ms response time | Should |

---

## Functional Requirements

### FR-1: Query mpv properties
- **Description**: Send `get_property` commands to mpv IPC socket for `media-title`, `playback-time`, `duration`, `pause`
- **Acceptance Criteria**: All 4 properties retrieved in a single socket connection
- **Priority**: Must

### FR-2: Human-readable output
- **Description**: Default output format: title, position (MM:SS / MM:SS), source, paused status
- **Acceptance Criteria**: Output matches spec example format
- **Priority**: Must

### FR-3: JSON output
- **Description**: `--json` flag emits `{"title","position","duration","paused","source"}` JSON
- **Acceptance Criteria**: Valid JSON parseable by jq/waybar
- **Priority**: Must

### FR-4: Query mpv property with response
- **Description**: New `query_mpv_property()` function that sends a get_property command and returns the response value. Extends existing IPC infrastructure.
- **Acceptance Criteria**: Returns typed serde_json::Value from mpv response
- **Priority**: Must

### FR-5: Not-playing detection
- **Description**: When no mpv socket exists or no file is loaded, exit with code 1 and empty/minimal output
- **Acceptance Criteria**: Exit 1 with no output (or `{"playing":false}` with `--json`)
- **Priority**: Must

### FR-6: CLI wiring
- **Description**: Add `Status` variant to Commands enum with `--json` flag
- **Acceptance Criteria**: `media-control status` and `media-control status --json` both work
- **Priority**: Must

---

## Non-Functional Requirements

### Performance
| Requirement | Metric | Target |
|-------------|--------|--------|
| Response time | Socket query round-trip | < 50ms |
| Suitability for polling | Waybar refresh interval | Every 1-5s |

---

## Constraints

- Reuse existing socket discovery logic (no Hyprland dependency)
- Single attempt, no retry, short timeout (200ms) — status should be fast or fail
- No new crate dependencies

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| mpv IPC supports multi-command per connection | Must send 4 get_property commands | Verified: mpv supports one JSON per line |
| media-title contains parseable show info | Title format varies | Show raw title, structured data via shim is optional future work |
