---
id: 003-cli-wiring
unit: 001-status-command
intent: 006-status-subcommand
status: complete
priority: must
created: 2026-03-19T19:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 003-cli-wiring

## User Story

**As a** media-control user
**I want** `status` wired as a CLI subcommand with `--json` flag
**So that** I can invoke it from scripts and status bars

## Acceptance Criteria

- [ ] **Given** `media-control status`, **When** invoked, **Then** routes to status command with json=false <!-- tw:85a7615b-d98a-4264-aadb-ef5669009663 -->
- [ ] **Given** `media-control status --json`, **When** invoked, **Then** routes with json=true <!-- tw:6a6dbe6b-d251-4ae2-85c2-dc9cc1ea755b -->
- [ ] **Given** `media-control --help`, **When** invoked, **Then** shows Status subcommand <!-- tw:892f044e-9e6c-4a81-a289-9da9a15215cb -->

## Technical Notes

- Add `Status { #[arg(long)] json: bool }` to Commands enum
- Add match arm routing to `commands::status::status(&ctx, json)`
- Add `pub mod status;` to commands/mod.rs
