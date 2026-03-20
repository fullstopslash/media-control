---
unit: 001-jellyfin-error-handling
intent: 007-jellyfin-error-handling
phase: inception
status: complete
created: 2026-03-19T20:00:00.000Z
updated: 2026-03-19T20:00:00.000Z
---

# Unit Brief: Jellyfin Error Handling

## Purpose

Harden Jellyfin HTTP client by surfacing HTTP error status codes and logging resume position failures. Prevents confusing JSON parse errors when the server returns 4xx/5xx.

## Scope

### In Scope
- Adding `.error_for_status()?` to all 8 GET request chains in jellyfin.rs
- Replacing silent `unwrap_or(0)` with logged error fallback in play.rs

### Out of Scope
- POST/DELETE request error handling (already adequate)
- Retry logic
- Custom error types

---

## Assigned Requirements

| FR | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Add error_for_status to GET requests | Must |
| FR-2 | Log resume ticks errors in play.rs | Must |

---

## Story Summary

| Metric | Count |
|--------|-------|
| Total Stories | 2 |
| Must Have | 2 |

### Stories

| Story ID | Title | Priority | Status |
|----------|-------|----------|--------|
| 001-get-error-status | Add error_for_status to GET requests | Must | Planned |
| 002-resume-error-logging | Log resume ticks errors in play.rs | Must | Planned |

---

## Dependencies

### Depends On
| Unit | Reason |
|------|--------|
| None | |

### External Dependencies
| System | Purpose | Risk |
|--------|---------|------|
| Jellyfin server | HTTP API | Low |

---

## Success Criteria

### Functional
- [ ] All 8 GET requests include `.error_for_status()?` <!-- tw:423fd6b0-5673-403b-be73-72d4f62352a7 -->
- [ ] Resume position errors logged to stderr <!-- tw:379999ad-88e9-473d-86bc-d2cd9a818c1d -->
- [ ] `cargo check`, `cargo clippy`, and `cargo test` pass <!-- tw:a354cb98-eeb2-4fff-b1b5-b8050bcee676 -->

---

## Bolt Suggestions

| Bolt | Type | Stories | Objective |
|------|------|---------|-----------|
| 012-jellyfin-error-handling | simple-construction-bolt | all 2 | Full error handling hardening |
