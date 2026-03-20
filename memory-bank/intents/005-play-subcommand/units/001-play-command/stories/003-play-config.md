---
id: 003-play-config
unit: 001-play-command
intent: 005-play-subcommand
status: complete
priority: must
created: 2026-03-19T18:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 003-play-config

## User Story

**As a** media-control user
**I want** a `[play]` config section with `pinchflat_library_id`
**So that** the Pinchflat library ID isn't hardcoded

## Acceptance Criteria

- [ ] **Given** config.toml has `[play]` with `pinchflat_library_id`, **When** config loads, **Then** `config.play.pinchflat_library_id` is Some(id) <!-- tw:1dd32b13-eff5-426a-9c97-b76cd7d1e3fd -->
- [ ] **Given** config.toml has no `[play]` section, **When** config loads, **Then** `config.play` defaults (no error) <!-- tw:c51138dc-d643-4145-94e7-3f2de8a2e084 -->
- [ ] **Given** no pinchflat_library_id and `recent-pinchflat` target, **When** play runs, **Then** it errors "No pinchflat_library_id in config" <!-- tw:6d473762-2758-48ae-97d1-dcb6385a8b98 -->

## Technical Notes

- `PlayConfig` struct with `#[serde(default)]` and `Option<String>` field
- Add `pub play: PlayConfig` to Config with `#[serde(default)]`

## Dependencies

### Requires
- None

### Enables
- 004-play-command (reads config)
