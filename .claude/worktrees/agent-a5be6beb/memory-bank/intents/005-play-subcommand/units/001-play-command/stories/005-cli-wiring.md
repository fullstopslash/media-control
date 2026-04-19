---
id: 005-cli-wiring
unit: 001-play-command
intent: 005-play-subcommand
status: complete
priority: must
created: 2026-03-19T18:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 005-cli-wiring

## User Story

**As a** media-control user
**I want** `play` wired into the CLI as a subcommand
**So that** I can invoke it from Hyprland keybindings

## Acceptance Criteria

- [-] **Given** `media-control play next-up`, **When** invoked, **Then** routes to `commands::play::play()` <!-- tw:a19cef07-69ea-40dd-b312-41bc21ea2cb7 -->
- [-] **Given** `media-control play` with no target, **When** invoked, **Then** clap shows usage error <!-- tw:940df282-70b9-4099-896a-862836e42cd9 -->
- [-] **Given** `media-control --help`, **When** invoked, **Then** shows Play subcommand in help text <!-- tw:f6b0852c-c2c7-4547-b63b-6791be0249f8 -->

## Technical Notes

- Add `Play { target: String }` to Commands enum in main.rs
- Add match arm routing to `commands::play::play(&ctx, &target)`
- Add `pub mod play;` to commands/mod.rs

## Dependencies

### Requires
- 004-play-command (the implementation to route to)

### Enables
- None (terminal story)
