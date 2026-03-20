---
id: 001-sigterm-handling
unit: 001-daemon-signals
intent: 008-daemon-reliability
status: complete
priority: must
created: 2026-03-19T00:00:00.000Z
assigned_bolt: null
implemented: true
---

# Story: 001-sigterm-handling

## User Story

**As a** system administrator
**I want** the daemon to handle SIGTERM gracefully
**So that** `media-control-daemon stop` cleans up PID file and FIFO reliably

## Acceptance Criteria

- [ ] **Given** the daemon is running in foreground, **When** SIGTERM is received, **Then** it logs "Received SIGTERM, shutting down" and exits cleanly <!-- tw:dd52cfbc-75e8-4643-b1f0-fd80b02d6244 -->
- [ ] **Given** the daemon is running, **When** `cmd_stop` sends SIGTERM, **Then** PID file and FIFO are removed <!-- tw:6a3364cd-bc20-4649-8d39-723fbb88c7aa -->
- [ ] **Given** the daemon is running, **When** SIGTERM is received, **Then** exit code is 0 (success) <!-- tw:064b68d5-ab75-4069-937f-723d629d80e7 -->

## Technical Notes

- Add `tokio::signal::unix::signal(SignalKind::terminate())` in `run_foreground`
- Add a new branch to the existing `tokio::select!` macro
- Existing cleanup code (remove_pid_file, remove_fifo) already runs after the select! block
- No new dependencies required — tokio already has the `signal` feature enabled
