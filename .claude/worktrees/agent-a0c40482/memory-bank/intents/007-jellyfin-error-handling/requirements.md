---
intent: 007-jellyfin-error-handling
phase: inception
status: complete
created: 2026-03-19T20:00:00.000Z
updated: 2026-03-19T20:00:00.000Z
---

# Requirements: Jellyfin Error Handling

## Intent Overview

Add `.error_for_status()?` to all Jellyfin GET requests and fix play.rs resume position error swallowing. Currently, HTTP error responses (4xx/5xx) are silently passed to JSON deserialization, producing confusing parse errors instead of clear HTTP status errors. The resume position lookup silently swallows errors with `unwrap_or(0)`.

## Business Goals

| Goal | Success Metric | Priority |
|------|----------------|----------|
| Surface HTTP errors from Jellyfin API | 4xx/5xx produce reqwest error, not JSON parse error | Must |
| Visible resume position failures | Errors logged to stderr when resume lookup fails | Must |

---

## Functional Requirements

### FR-1: Add error_for_status to GET requests
- **Description**: Add `.error_for_status()?` before `.json()` on all GET requests in jellyfin.rs (lines 391, 486, 500, 513, 675, 682, 723, 736)
- **Acceptance Criteria**: All 8 GET request chains include `.error_for_status()?` between `.send().await?` and `.json().await?`
- **Priority**: Must

### FR-2: Log resume ticks errors in play.rs
- **Description**: In play.rs, log the error when `get_item_resume_ticks` fails instead of silently using `unwrap_or(0)`
- **Acceptance Criteria**: Failed resume lookups emit an eprintln message and fall back to 0
- **Priority**: Must

---

## Non-Functional Requirements

### Reliability
| Requirement | Metric | Target |
|-------------|--------|--------|
| Error clarity | HTTP errors surface as status errors | 100% of GET requests |

---

## Constraints

- No new crate dependencies
- No behavioral changes for successful requests

---

## Assumptions

| Assumption | Risk if Invalid | Mitigation |
|------------|-----------------|------------|
| All GET requests should fail on HTTP errors | Some endpoints may return non-200 legitimately | Review each endpoint; all 8 are standard data fetches |
