---
id: 001-get-error-status
unit: 001-jellyfin-error-handling
intent: 007-jellyfin-error-handling
status: complete
priority: must
created: 2026-03-19T20:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 001-get-error-status

## User Story

**As a** media-control user
**I want** HTTP error responses from Jellyfin to produce clear error messages
**So that** I see "401 Unauthorized" instead of "expected value at line 1 column 1"

## Acceptance Criteria

- [ ] **Given** a GET request returns 401, **When** the response is processed, **Then** it returns a reqwest status error <!-- tw:7edd8323-bce6-4df9-9e46-11bcf817d84d -->
- [ ] **Given** a GET request returns 200, **When** the response is processed, **Then** behavior is unchanged <!-- tw:06ce4293-30ab-4330-9fce-edb67119da99 -->
- [ ] **Given** all 8 GET request sites in jellyfin.rs, **When** reviewed, **Then** all include `.error_for_status()?` <!-- tw:4f4d8256-e4e8-469d-8251-88b1178dbda7 -->

## Technical Notes

- Change `.send().await?.json().await?` to `.send().await?.error_for_status()?.json().await?`
- Affected lines: 391, 486, 500, 513, 675, 682, 723, 736
- `error_for_status()` consumes the response and returns `Err` for 4xx/5xx
