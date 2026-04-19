---
id: 002-resume-error-logging
unit: 001-jellyfin-error-handling
intent: 007-jellyfin-error-handling
status: complete
priority: must
created: 2026-03-19T20:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 002-resume-error-logging

## User Story

**As a** media-control user
**I want** resume position lookup failures to be logged
**So that** I know when playback starts from the beginning due to an error

## Acceptance Criteria

- [-] **Given** `get_item_resume_ticks` fails, **When** play command runs, **Then** error is logged to stderr <!-- tw:17fccac2-3090-49bf-b9fd-9836d84a678b -->
- [-] **Given** `get_item_resume_ticks` fails, **When** play command runs, **Then** playback still starts from position 0 <!-- tw:18b86219-2111-43ed-a27c-0a22951f820d -->
- [-] **Given** `get_item_resume_ticks` succeeds, **When** play command runs, **Then** behavior is unchanged <!-- tw:03ce634b-0ff0-48ed-b6b6-f4879759fe79 -->

## Technical Notes

- Replace `jf.get_item_resume_ticks(&item_id).await.unwrap_or(0)` with a match expression
- Log format: `media-control: failed to get resume position (starting from beginning): {e}`
- Non-fatal: playback should proceed with position 0
