---
id: 002-status-command
unit: 001-status-command
intent: 006-status-subcommand
status: complete
priority: must
created: 2026-03-19T19:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 002-status-command

## User Story

**As a** media-control user
**I want** `media-control status` to show current playback state
**So that** I can check what's playing and integrate with my status bar

## Acceptance Criteria

- [ ] **Given** mpv is playing, **When** `status` runs, **Then** it prints title, position, duration, pause state <!-- tw:a8a953c9-42c6-4884-a3e6-f85a0f9a6ec0 -->
- [ ] **Given** mpv is playing, **When** `status --json` runs, **Then** it emits valid JSON with all fields <!-- tw:93f6959f-4340-43dd-aea8-ff44acaff65b -->
- [ ] **Given** no mpv socket, **When** `status` runs, **Then** it exits with code 1 <!-- tw:69dfdebc-45d5-410f-9751-42669f8dcb76 -->
- [ ] **Given** `--json` and not playing, **When** `status --json` runs, **Then** it emits `{"playing":false}` <!-- tw:3662de61-6062-442c-aef5-f523827c8751 -->

## Technical Notes

- Query 4 properties: media-title, playback-time, duration, pause
- Human format: "Playing: {title}\nPosition: {mm:ss} / {mm:ss}\nPaused: {yes/no}"
- JSON: serde_json::json! with all fields
- Format seconds as MM:SS for human output
