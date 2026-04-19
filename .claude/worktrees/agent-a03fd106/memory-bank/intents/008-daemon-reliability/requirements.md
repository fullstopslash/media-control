---
intent: 008-daemon-reliability
phase: inception
status: complete
created: 2026-03-19T00:00:00Z
updated: 2026-03-19T00:00:00Z
---

# Requirements: Daemon Reliability

## Intent Overview

Improve daemon reliability with proper SIGTERM handling and graceful shutdown. When `cmd_stop` sends SIGTERM to the daemon process, the daemon should catch the signal, clean up its PID file and FIFO, and exit cleanly.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Clean daemon shutdown on SIGTERM | PID file and FIFO removed after stop | Must |
| No orphaned resources | No stale PID files or FIFOs after daemon stop | Must |

---

## Functional Requirements

### FR-1: Handle SIGTERM in daemon
- **Description**: Handle SIGTERM in the daemon so `cmd_stop` triggers clean shutdown (PID file + FIFO cleanup)
- **Acceptance Criteria**: `media-control-daemon stop` causes the daemon to exit cleanly, removing PID file and FIFO
- **Priority**: Must

### FR-2: Add SIGTERM to select! branches
- **Description**: Add `tokio::signal::unix::signal(SignalKind::terminate())` as a branch in the foreground `select!`
- **Acceptance Criteria**: SIGTERM is caught alongside SIGINT in the foreground event loop
- **Priority**: Must

---

## Non-Functional Requirements

### Reliability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Signal handling latency | Time from SIGTERM to cleanup | < 100ms |

---

## Constraints

- No new crate dependencies
- Reuse existing cleanup functions (remove_pid_file, remove_fifo)

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| tokio::signal::unix available on Linux | Won't compile on non-Unix | This project targets Linux only |
