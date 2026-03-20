---
unit: 001-daemon-signals
intent: 008-daemon-reliability
phase: inception
status: complete
created: 2026-03-19T00:00:00Z
updated: 2026-03-19T00:00:00Z
---

# Unit Brief: Daemon Signals

## Purpose

Add SIGTERM handling to the daemon's foreground select! loop so that `cmd_stop` (which sends SIGTERM) triggers a clean shutdown with PID file and FIFO cleanup.

## Scope

### In Scope
- SIGTERM signal handler registration
- SIGTERM branch in the foreground select! macro
- Clean shutdown path (reuses existing cleanup)

### Out of Scope
- SIGHUP reload
- Watchdog / health checks
- Restart logic

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Handle SIGTERM in daemon | Must |
| FR-2 | Add SIGTERM to select! branches | Must |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 1 |
| Must Have | 1 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-sigterm-handling | Handle SIGTERM for clean daemon shutdown | Must | Planned |

---

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| None | Self-contained change |

### External Dependencies
| System | Purpose | Risk |
|--------|---------|------|
| tokio::signal::unix | SIGTERM registration | Low (Linux only) |

---

## Success Criteria

### Functional
- [ ] `media-control-daemon stop` triggers clean shutdown <!-- tw:3602cd04-9a3d-487a-bb52-54bae8252c93 -->
- [ ] PID file removed after SIGTERM <!-- tw:7c9b8594-8cdf-422d-b218-a45cc062abba -->
- [ ] FIFO removed after SIGTERM <!-- tw:b240cc6d-4c23-47ec-977b-10fe09168241 -->

### Non-Functional
- [ ] No new dependencies <!-- tw:e3fd571a-ca47-4556-a14c-94bf9ccc2391 -->

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 013-daemon-signals | simple-construction-bolt | 001-sigterm-handling | SIGTERM handling |
