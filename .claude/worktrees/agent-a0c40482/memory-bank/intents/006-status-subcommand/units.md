---
intent: 006-status-subcommand
phase: inception
status: units-decomposed
updated: 2026-03-19T19:00:00Z
---

# Status Subcommand - Unit Decomposition

## Units Overview

1 unit. All FRs are tightly coupled — query properties, format output, wire CLI.

### Unit 1: status-command

**Description**: Query mpv IPC for playback properties and output human-readable or JSON status.

**Stories**:
- 001-query-mpv-property: New function to query mpv and return response value
- 002-status-command: status.rs command module with property querying + formatting
- 003-cli-wiring: Wire Status variant into main.rs

**Dependencies**: Depends on intent 004 IPC hardening (socket discovery, validation)

**Estimated Complexity**: S

## Requirement-to-Unit Mapping

- **FR-1–FR-6**: All → `001-status-command`
