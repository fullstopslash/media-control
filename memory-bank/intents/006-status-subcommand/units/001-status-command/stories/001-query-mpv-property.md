---
id: 001-query-mpv-property
unit: 001-status-command
intent: 006-status-subcommand
status: complete
priority: must
created: 2026-03-19T19:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 001-query-mpv-property

## User Story

**As a** media-control developer
**I want** a `query_mpv_property()` function that returns mpv property values
**So that** the status command can read playback state

## Acceptance Criteria

- [ ] **Given** mpv is playing, **When** `query_mpv_property("media-title")` is called, **Then** it returns the title as <!-- tw:29815a6a-45dd-4b22-82d1-184707b15fdd -->
- [ ] **Given** mpv socket doesn't exist, **When** query is attempted, **Then** it returns an error <!-- tw:c60596c2-5abc-4b80-a94e-3de17a660e27 -->
- [ ] **Given** mpv returns `{"error":"success","data":"Some Title"}`, **When** parsed, **Then** it extracts the `data` field <!-- tw:f26614ca-a571-434f-b346-1281ca957d92 -->

## Technical Notes

- Reuse socket discovery from send_mpv_ipc_command but return the response instead of discarding it
- Send `{"command":["get_property","<name>"]}\n`, read one response line
- Parse response JSON, extract `data` field
- Single attempt, no retry, 200ms timeout — status should be fast or fail
