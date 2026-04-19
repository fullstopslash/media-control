---
id: 001-config-fallback
unit: 001-config-window-fixes
intent: 010-config-window-hardening
status: complete
priority: must
created: 2026-03-19T12:00:00.000Z
assigned_bolt: 015-config-window-fixes
implemented: true
---

# Story: 001-config-fallback

## User Story

**As a** media-control user
**I want** the CLI to fall back to default config when no config file exists
**So that** I can use media-control without creating a config file first

## Acceptance Criteria

- [ ] **Given** no config file exists and no --config flag, **When** media-control runs, **Then** it uses successfully <!-- tw:144fe534-0618-468e-ab47-7fe024eaf310 -->
- [-] **Given** a config file with parse errors, **When** media-control runs without --config, **Then** it falls back to defaults and logs a debug message <!-- tw:c5e43e72-2526-4144-ae5c-84e88a57147c -->
- [-] **Given** --config points to a missing file, **When** media-control runs, **Then** it returns an error (explicit path should not silently fall back) <!-- tw:525c10ab-6ca5-4c74-89f9-8da5fa5dd7af -->

## Technical Notes

- Change `Config::load()?` to `Config::load().unwrap_or_else(...)` in `run()` in main.rs
- Use `tracing::debug!` for the fallback log message
- Only the `None` (auto-discovery) branch falls back; explicit `--config` path still errors

## Dependencies

### Requires
- None

### Enables
- None
